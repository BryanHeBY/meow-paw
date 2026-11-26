//! Local inspection of an already fetched rule snapshot.

use crate::engine::controller::Rule;
use unicase::UniCase;

/// Return original one-based positions and rules in their snapshot order.
pub fn matching_rules<'a>(rules: &'a [Rule], query: &str) -> Vec<(usize, &'a Rule)> {
    let query = if query.trim().is_empty() {
        String::new()
    } else {
        UniCase::new(query).to_folded_case()
    };
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            query.is_empty()
                || UniCase::new(&rule.payload)
                    .to_folded_case()
                    .contains(&query)
        })
        .map(|(index, rule)| (index + 1, rule))
        .collect()
}

/// Clamp a requested zero-based page after a snapshot changes.
pub fn page_bounds(total: usize, page: usize, page_size: usize) -> (usize, usize, usize) {
    assert!(page_size > 0);
    let page = page.min(total.saturating_sub(1) / page_size);
    let start = page * page_size;
    (page, start, start.saturating_add(page_size).min(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(payload: &str, kind: &str, proxy: &str) -> Rule {
        Rule {
            kind: kind.into(),
            payload: payload.into(),
            proxy: proxy.into(),
        }
    }

    #[test]
    fn filters_payload_with_unicode_case_folding_and_original_positions() {
        let rules = vec![
            rule("one.Example", "DOMAIN", "DIRECT"),
            rule("Straße.测试", "DOMAIN", "PROXY"),
            rule("other", "STRASSE", "example"),
            rule("STRASSE.example", "DOMAIN", "DIRECT"),
            rule("", "MATCH", "DIRECT"),
        ];
        let positions = |query| {
            matching_rules(&rules, query)
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(""), vec![1, 2, 3, 4, 5]);
        assert_eq!(positions("  "), vec![1, 2, 3, 4, 5]);
        assert_eq!(positions("eXaMpLe"), vec![1, 4]);
        assert_eq!(positions("strasse"), vec![2, 4]);
        assert_eq!(positions("测试"), vec![2]);
        assert_eq!(positions("DOMAIN"), Vec::<usize>::new());
        assert_eq!(positions("missing"), Vec::<usize>::new());
        assert_eq!(positions(" example"), Vec::<usize>::new());
        assert_eq!(rules[1].payload, "Straße.测试");
    }

    #[test]
    fn pages_cover_all_results_and_clamp_after_refresh() {
        assert_eq!(page_bounds(0, 0, 100), (0, 0, 0));
        assert_eq!(page_bounds(201, 0, 100), (0, 0, 100));
        assert_eq!(page_bounds(201, 1, 100), (1, 100, 200));
        assert_eq!(page_bounds(201, 2, 100), (2, 200, 201));
        assert_eq!(page_bounds(50, 2, 100), (0, 0, 50));
    }
}
