"""In-memory acceptance fixture for the TUN ownership contract."""

from dataclasses import dataclass, field
from enum import Enum, auto
import unittest


class Conflict(Enum):
    CLEAR = auto()
    ACTIVE = auto()
    UNKNOWN = auto()


class OwnerStatus(Enum):
    ALIVE = auto()
    DEAD = auto()
    UNKNOWN = auto()


class Refused(Exception):
    pass


class Interrupted(Exception):
    pass


@dataclass(frozen=True)
class Effect:
    key: str
    generation: int
    value: str


@dataclass
class Journal:
    generation: int
    pending: list[Effect] = field(default_factory=list)


@dataclass
class Store:
    next_generation: int = 1
    journal: Journal | None = None


@dataclass
class MockAdapter:
    rows: dict[str, Effect] = field(default_factory=dict)
    conflict: Conflict = Conflict.CLEAR
    foreign_users: set[str] = field(default_factory=set)
    removed: list[Effect] = field(default_factory=list)

    def add_if_absent(self, effect):
        if self.conflict is not Conflict.CLEAR or effect.key in self.rows:
            return False
        self.rows[effect.key] = effect
        return True

    def remove_if_owned(self, effect):
        current = self.rows.get(effect.key)
        if current is None:
            return True
        if current != effect or effect.key in self.foreign_users:
            return False
        del self.rows[effect.key]
        self.removed.append(effect)
        return True

    def ready(self, effects):
        return self.conflict is Conflict.CLEAR and all(
            self.rows.get(effect.key) == effect for effect in effects
        )


class Coordinator:
    """The Store outlives the coordinator. Tests serialize coordinator calls."""

    def __init__(self, store, adapter):
        self.store = store
        self.adapter = adapter

    def preflight(self):
        if self.adapter.conflict is not Conflict.CLEAR:
            raise Refused("foreign capture is active or unknown")

    def start(self, plan, crash_at=None):
        self.preflight()
        if self.store.journal is not None:
            raise Refused("prior generation still has a journal")
        generation = self.store.next_generation
        self.store.next_generation += 1
        journal = self.store.journal = Journal(generation)
        try:
            for index, (key, value) in enumerate(plan):
                effect = Effect(key, generation, value)
                journal.pending.append(effect)
                if crash_at == (index, "before"):
                    raise Interrupted
                if not self.adapter.add_if_absent(effect):
                    # The mock's conditional create proves it made no write.
                    journal.pending.pop()
                    raise Refused(f"conditional create failed for {key}")
                if crash_at == (index, "after"):
                    raise Interrupted
            if not self.adapter.ready(journal.pending):
                raise Refused("capture readiness could not be confirmed")
        except Refused:
            self.stop(generation)
            raise
        return generation

    def stop(self, generation):
        journal = self.store.journal
        if journal is None or journal.generation != generation:
            return False
        while journal.pending:
            effect = journal.pending[-1]
            if not self.adapter.remove_if_owned(effect):
                raise Refused(f"ownership or dependency unclear for {effect.key}")
            journal.pending.pop()
        self.store.journal = None
        return True

    def recover(self, owner_status):
        journal = self.store.journal
        if journal is None:
            return False
        if owner_status is not OwnerStatus.DEAD:
            raise Refused("previous owner may still be live")
        return self.stop(journal.generation)

    def restart(self, generation, plan):
        self.preflight()
        if not self.stop(generation):
            raise Refused("restart belongs to a different generation")
        return self.start(plan)


# The shared-settings questions are represented by separate, synthetic rows.
# The fixture has no facility for replacing a shared host setting.
PLAN = (("adapter", "device"), ("route", "capture"), ("dns", "binding"))
CLASH = {"clash": Effect("clash", 0, "third-party")}


class TunOwnershipTests(unittest.TestCase):
    def setUp(self):
        self.store = Store()
        self.adapter = MockAdapter()
        self.controller = Coordinator(self.store, self.adapter)

    def test_active_or_unknown_conflict_denies_start_without_changes(self):
        self.adapter.rows.update(CLASH)
        for conflict in (Conflict.ACTIVE, Conflict.UNKNOWN):
            with self.subTest(conflict=conflict):
                self.adapter.conflict = conflict
                with self.assertRaisesRegex(Refused, "foreign capture"):
                    self.controller.start(PLAN)
                self.assertEqual(self.adapter.rows, CLASH)
                self.assertEqual(self.store, Store())
                self.assertEqual(self.adapter.removed, [])

    def test_collision_rolls_back_only_applied_effects(self):
        foreign = Effect("route", 0, "capture")
        self.adapter.rows[foreign.key] = foreign
        with self.assertRaisesRegex(Refused, "conditional create failed"):
            self.controller.start(PLAN)
        self.assertEqual(self.adapter.rows, {"route": foreign})
        self.assertEqual([effect.key for effect in self.adapter.removed], ["adapter"])
        self.assertIsNone(self.store.journal)

    def test_conflict_at_readiness_does_not_commit_and_preserves_foreign_capture(self):
        class ConflictAtReadiness(MockAdapter):
            def ready(self, effects):
                self.conflict = Conflict.ACTIVE
                self.rows.update(CLASH)
                return super().ready(effects)

        adapter = ConflictAtReadiness()
        with self.assertRaisesRegex(Refused, "readiness could not be confirmed"):
            Coordinator(self.store, adapter).start(PLAN)
        self.assertEqual(adapter.rows, CLASH)
        self.assertEqual([effect.key for effect in adapter.removed], ["dns", "route", "adapter"])
        self.assertIsNone(self.store.journal)

    def test_crash_before_and_after_each_write_recovers_exactly(self):
        for index in range(len(PLAN)):
            for phase in ("before", "after"):
                with self.subTest(index=index, phase=phase):
                    store = Store()
                    baseline = Effect("proxy", 0, "external setting")
                    adapter = MockAdapter(rows={"proxy": baseline})
                    with self.assertRaises(Interrupted):
                        Coordinator(store, adapter).start(PLAN, (index, phase))
                    expected = index + (phase == "after")
                    self.assertEqual(len(adapter.rows), 1 + expected)
                    recovery = Coordinator(store, adapter)
                    self.assertTrue(recovery.recover(OwnerStatus.DEAD))
                    self.assertFalse(recovery.recover(OwnerStatus.DEAD))
                    self.assertEqual(adapter.rows, {"proxy": baseline})
                    self.assertEqual(adapter.removed, [
                        Effect(key, 1, value) for key, value in reversed(PLAN[:expected])
                    ])
                    self.assertIsNone(store.journal)

    def test_recovery_requires_proof_owner_died(self):
        generation = self.controller.start(PLAN)
        before = self.adapter.rows.copy()
        for status in (OwnerStatus.ALIVE, OwnerStatus.UNKNOWN):
            with self.subTest(status=status):
                with self.assertRaisesRegex(Refused, "may still be live"):
                    Coordinator(self.store, self.adapter).recover(status)
                self.assertEqual(self.adapter.rows, before)
                self.assertEqual(self.store.journal.generation, generation)

    def test_recovery_preserves_foreign_replacement_and_blocks_start(self):
        self.controller.start(PLAN)
        self.adapter.rows["dns"] = Effect("dns", 0, "binding")
        before = self.adapter.rows.copy()
        recovery = Coordinator(self.store, self.adapter)
        with self.assertRaisesRegex(Refused, "unclear for dns"):
            recovery.recover(OwnerStatus.DEAD)
        with self.assertRaisesRegex(Refused, "prior generation"):
            recovery.start(PLAN)
        self.assertEqual(self.adapter.rows, before)
        self.assertEqual(self.adapter.removed, [])

    def test_stop_preserves_an_owned_resource_with_a_foreign_user(self):
        generation = self.controller.start(PLAN)
        self.adapter.conflict = Conflict.ACTIVE
        self.adapter.rows.update(CLASH)
        self.adapter.foreign_users.add("adapter")
        with self.assertRaisesRegex(Refused, "unclear for adapter"):
            self.controller.stop(generation)
        self.assertEqual(self.adapter.rows, CLASH | {"adapter": Effect("adapter", generation, "device")})
        self.adapter.foreign_users.clear()
        self.assertTrue(self.controller.stop(generation))
        self.assertFalse(self.controller.stop(generation))
        self.assertEqual(self.adapter.rows, CLASH)

    def test_stop_retries_after_effect_removed_before_journal_updated(self):
        generation = self.controller.start(PLAN)
        last = self.store.journal.pending[-1]
        self.assertTrue(self.adapter.remove_if_owned(last))
        self.assertTrue(Coordinator(self.store, self.adapter).recover(OwnerStatus.DEAD))
        self.assertEqual(self.adapter.removed, [
            Effect(key, generation, value) for key, value in reversed(PLAN)
        ])
        self.assertEqual(self.adapter.rows, {})

    def test_restart_preflight_preserves_running_generation_and_fences_old_stop(self):
        generation = self.controller.start(PLAN)
        before = self.adapter.rows.copy()
        self.adapter.conflict = Conflict.ACTIVE
        with self.assertRaisesRegex(Refused, "foreign capture"):
            self.controller.restart(generation, PLAN)
        self.assertEqual(self.adapter.rows, before)
        self.assertEqual(self.store.journal.generation, generation)
        self.assertEqual(self.adapter.removed, [])

        self.adapter.conflict = Conflict.CLEAR
        replacement = self.controller.restart(generation, PLAN)
        self.assertGreater(replacement, generation)
        self.assertFalse(self.controller.stop(generation))
        self.assertEqual(self.adapter.rows, {
            key: Effect(key, replacement, value) for key, value in PLAN
        })


if __name__ == "__main__":
    unittest.main()
