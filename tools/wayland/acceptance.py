#!/usr/bin/env python3
"""Opt-in headless Sway tray/focus probe. Never connects to the user's desktop."""
import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from urllib.parse import unquote
from xml.sax.saxutils import escape

REQUIRED = ("sway", "swaymsg", "fcitx5", "fcitx5-remote", "dbus-run-session", "dbus-daemon", "busctl", "unshare")
PROFILE = "mode: rule\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n"


def dependency_diagnostics(which=shutil.which):
    return [name for name in REQUIRED if not which(name)]


def synthetic_config(root):
    """Create a fresh, private test tree; never reuse existing configuration."""
    root = Path(root)
    home = root / "home"
    home.mkdir(mode=0o700)  # fail closed if already present, including symlinks
    for directory in (home / ".config", home / ".cache", home / ".local" / "share",
                      home / ".local" / "state", root / "runtime", root / "system-config"):
        directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    profile = root / "profile.yaml"
    with profile.open("x", encoding="utf-8") as output:
        output.write(PROFILE)
    profile.chmod(0o600)
    return profile


def environment(root):
    root = Path(root)
    home = root / "home"
    return {
        "PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"), "XDG_CONFIG_DIRS": str(root / "system-config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_DATA_HOME": str(home / ".local" / "share"),
        "XDG_STATE_HOME": str(home / ".local" / "state"), "XDG_RUNTIME_DIR": str(root / "runtime"),
        "TMPDIR": str(root / "runtime"),
        "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/nonexistent/paw-no-system-bus",
        "WLR_BACKENDS": "headless", "WLR_RENDERER": "pixman", "WLR_LIBINPUT_NO_DEVICES": "1",
        "QT_QPA_PLATFORM": "wayland", "GDK_BACKEND": "wayland",
        "GTK_IM_MODULE": "fcitx", "QT_IM_MODULE": "fcitx", "XMODIFIERS": "@im=fcitx",
    }


def sni_registered(output, service):
    try:
        fields = shlex.split(output)
        return (len(fields) >= 2 and fields[0] == "as" and fields[1].isdigit()
                and int(fields[1]) == len(fields) - 2 and service in fields[2:])
    except ValueError:
        return False


def paw_views(tree, app_pid):
    result = []
    def walk(node):
        if not isinstance(node, dict):
            return
        if node.get("pid") == app_pid and node.get("type") in ("con", "floating_con"):
            result.append(node)
        for child in node.get("nodes", []) + node.get("floating_nodes", []):
            walk(child)
    walk(tree)
    return result


def focused_paw(tree, app_pid):
    views = paw_views(tree, app_pid)
    return len(views) == 1 and views[0].get("focused") is True


def command(args, env, check=True):
    return subprocess.run(args, env=env, text=True, capture_output=True, timeout=5, check=check)


def eventually(probe, label, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(0.1)
    raise RuntimeError("timed out: " + label)


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=4)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=4)


def outer_is_ancestor(pid):
    current = os.getppid()
    while current > 1:
        if current == pid:
            return True
        status = Path(f'/proc/{current}/status').read_text(encoding='utf-8')
        current = int(next(line.split()[1] for line in status.splitlines() if line.startswith('PPid:')))
    return False


def inner(root, app, watcher, outer_pid, user_fd, net_fd):
    # Compare inherited outer namespace descriptors, since accessing outer /proc
    # namespace links can be denied after entering a user namespace.
    try:
        same_user = os.stat('/proc/self/ns/user').st_ino == os.fstat(user_fd).st_ino
        same_net = os.stat('/proc/self/ns/net').st_ino == os.fstat(net_fd).st_ino
    except OSError as error:
        raise RuntimeError('outer namespace descriptors unavailable; isolated launch cannot be verified') from error
    if not outer_is_ancestor(outer_pid) or same_user or same_net:
        raise RuntimeError('isolated user/network namespace required')
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    if not address.startswith('unix:path=') or ';' in address:
        raise RuntimeError('private session bus required')
    socket_path = Path(unquote(address[len('unix:path='):].split(',', 1)[0]))
    if socket_path.parent != root / 'runtime':
        raise RuntimeError('session bus must be in the private runtime directory')
    env = environment(root)
    env['DBUS_SESSION_BUS_ADDRESS'] = address
    config = root / 'sway.conf'
    config.write_text('output * resolution 1024x768\n', encoding='utf-8')
    processes = []
    def launch(args):
        proc = subprocess.Popen(args, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        processes.append(proc)
        return proc
    try:
        compositor = launch(['sway', '--config', str(config)])
        def sockets():
            if compositor.poll() is not None:
                raise RuntimeError('headless sway exited before creating sockets')
            runtime = root / 'runtime'
            ipc = list(runtime.glob('sway-ipc.*.sock'))
            wayland = [p for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')]
            return (ipc[0], wayland[0]) if len(ipc) == len(wayland) == 1 else None
        ipc, wayland = eventually(sockets, 'headless sway sockets')
        env.update(SWAYSOCK=str(ipc), WAYLAND_DISPLAY=wayland.name)
        fcitx = launch(['fcitx5'])
        def fcitx_ready():
            if fcitx.poll() is not None:
                raise RuntimeError('fcitx5 exited before becoming ready')
            result = command(['fcitx5-remote'], env, check=False)
            return result.returncode == 0 and result.stdout.strip() in ('1', '2')
        eventually(fcitx_ready, 'fcitx5 D-Bus service')
        tray = launch([str(watcher)])
        paw = launch([str(app), '--no-preferences', '--profile', str(root / 'profile.yaml')])
        service = f'org.kde.StatusNotifierItem-{paw.pid}-1'
        def alive():
            if paw.poll() is not None or tray.poll() is not None:
                raise RuntimeError('Paw or tray watcher exited unexpectedly')
        def tree():
            alive()
            return json.loads(command(['swaymsg', '-t', 'get_tree', '-r'], env).stdout)
        def registered():
            alive()
            result = command(['busctl', '--user', 'get-property', 'org.kde.StatusNotifierWatcher',
                              '/StatusNotifierWatcher', 'org.kde.StatusNotifierWatcher',
                              'RegisteredStatusNotifierItems'], env, check=False)
            return result.returncode == 0 and sni_registered(result.stdout, service)
        eventually(registered, 'SNI registration')
        eventually(lambda: len(paw_views(tree(), paw.pid)) == 1, 'initial Paw window')
        result = command(['swaymsg', f'[pid="{paw.pid}"]', 'kill'], env)
        if not all(reply.get('success') for reply in json.loads(result.stdout)):
            raise RuntimeError('sway did not accept the window close command')
        eventually(lambda: len(paw_views(tree(), paw.pid)) == 0, 'initial Paw window closed')
        def activate():
            command(['busctl', '--user', 'call', service, '/StatusNotifierItem',
                     'org.kde.StatusNotifierItem', 'Activate', 'ii', '0', '0'], env)
        activate()
        eventually(lambda: len(paw_views(tree(), paw.pid)) == 1, 'tray reopened Paw window')
        for _ in range(3):
            activate()
        time.sleep(0.5)
        if len(paw_views(tree(), paw.pid)) != 1:
            raise RuntimeError('repeated activation did not leave exactly one Paw window')
        eventually(lambda: focused_paw(tree(), paw.pid), 'reopened Paw window focus')
        print('PASS: headless SNI registration, close/reopen, duplicate guard and focus')
        print('LIMIT: Fcitx5 service started; actual IME composition was not tested')
    finally:
        for process in reversed(processes):
            stop(process)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group(required=True)
    actions.add_argument('--check', action='store_true', help='report missing host tools without launching the desktop')
    actions.add_argument('--run', action='store_true', help='opt in to the isolated headless smoke test')
    actions.add_argument('--inner', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--app', type=Path, default=Path('target/debug/meow-paw'))
    parser.add_argument('--watcher', type=Path, default=Path('target/debug/examples/tray_test_watcher'))
    parser.add_argument('--root', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--outer-pid', type=int, help=argparse.SUPPRESS)
    parser.add_argument('--user-fd', type=int, help=argparse.SUPPRESS)
    parser.add_argument('--net-fd', type=int, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.inner:
        if args.root is None or args.outer_pid is None or args.user_fd is None or args.net_fd is None:
            parser.error('internal arguments missing')
        inner(args.root, args.app, args.watcher, args.outer_pid, args.user_fd, args.net_fd)
        return 0
    missing = dependency_diagnostics(lambda name: shutil.which(name, path="/usr/bin:/bin"))
    if missing:
        print('Missing dependencies: ' + ', '.join(missing), file=sys.stderr)
        return 2
    if args.check:
        print('Required executable dependencies found; namespace and runtime behavior not yet verified')
        return 0
    if os.geteuid() == 0:
        raise RuntimeError('run as an unprivileged user')
    app, watcher = args.app.resolve(), args.watcher.resolve()
    for path in (app, watcher):
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RuntimeError(f'missing executable: {path}')
    with tempfile.TemporaryDirectory(prefix='paw-wayland-') as directory:
        root = Path(directory)
        synthetic_config(root)
        bus_config = root / 'session.conf'
        socket = escape(str(root / 'runtime' / 'bus'))
        bus_config.write_text(
            '<busconfig><type>session</type><listen>unix:path=' + socket + '</listen>'
            '<auth>EXTERNAL</auth><policy context="default">'
            '<allow send_destination="*"/><allow receive_sender="*"/>'
            '<allow own="*"/></policy></busconfig>', encoding='utf-8')
        user_fd = os.open('/proc/self/ns/user', os.O_RDONLY)
        net_fd = os.open('/proc/self/ns/net', os.O_RDONLY)
        cmd = ['unshare', '--user', '--map-current-user', '--net', 'dbus-run-session', '--config-file=' + str(bus_config), '--',
               sys.executable, str(Path(__file__).resolve()), '--inner', '--root', str(root),
               '--app', str(app), '--watcher', str(watcher), '--outer-pid', str(os.getpid()),
               '--user-fd', str(user_fd), '--net-fd', str(net_fd)]
        try:
            proc = subprocess.Popen(cmd, env=environment(root), start_new_session=True,
                                    pass_fds=(user_fd, net_fd))
        finally:
            os.close(user_fd)
            os.close(net_fd)
        try:
            return proc.wait(timeout=90)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            raise RuntimeError('isolated session timed out')
        finally:
            # Reap any descendants still in this isolated process group.
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        print(f'Wayland acceptance failed: {error}', file=sys.stderr)
        sys.exit(1)
