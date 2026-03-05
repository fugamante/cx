pub fn parse_labeled_u64(s: &str, label: &str) -> Option<u64> {
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(label) {
            return rest.trim().parse::<u64>().ok();
        }
    }
    None
}
