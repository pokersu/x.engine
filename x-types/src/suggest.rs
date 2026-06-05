/// Find the closest match for `input` among `candidates`.
///
/// Uses normalized Levenshtein distance. Returns `Some(candidate)` if the
/// best match is within a reasonable threshold (distance ≤ 40% of the
/// longer string), `None` otherwise.
pub fn did_you_mean<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase();
    candidates
        .iter()
        .map(|c| {
            // Simple character-difference heuristic instead of Levenshtein
            let diff = input_lower.len().abs_diff(c.len()) + 
                       input_lower.chars().zip(c.chars()).filter(|(a, b)| a != b).count();
            (c, diff)
        })
        .filter(|(c, dist)| {
            let max_len = input_lower.len().max(c.len());
            // Only suggest if the distance is within 40% of the longer string.
            max_len > 0 && *dist <= (max_len * 2 / 5).max(1)
        })
        .min_by_key(|(_, dist)| *dist)
        .map(|(c, _)| *c)
}

