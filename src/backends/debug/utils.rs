// replace these by regular expressions
pub fn get_next(str: &str, prepend: &str, breakers: Vec<char>) -> Option<String> {
    if str.starts_with(prepend) {
        let start = prepend.len();
        let mut end = start;
        loop {
            let nth = &str.chars().nth(end);
            if nth.is_none() {
                return None;
            }
            if breakers.contains(&nth.unwrap()) {
                break;
            }
            end += 1;
        }
        Some(str[start..end].to_owned())
    } else {
        None
    }
}

// replace these by regular expressions
pub fn get_after_dot(str: &str, breakers: Vec<char>) -> Option<String> {
    let mut start = 0;
    let mut dot_found = false;
    let mut end = start;
    loop {
        let nth = &str.chars().nth(end);
        if nth.is_none() {
            return None;
        }
        if nth.unwrap() == '.' {
            start = end + 1;
            dot_found = true;
        }
        if dot_found && breakers.contains(&nth.unwrap()) {
            break;
        }
        end += 1;
    }
    Some(str[start..end].to_owned())
}

mod test {
    #[test]
    fn test_get_after_dot_1() {
        let result = super::get_after_dot(
            "uint256(0x0000000000000000000000000000000000000000000000000000000000000000).toField()",
            vec!['('],
        );
        assert_eq!(result, Some("toField".to_owned()));
    }

    #[test]
    fn test_get_after_dot_2() {
        let result = super::get_after_dot("key.hooks.isValidHookAddress(key.fee)", vec!['(']);
        assert_eq!(result, Some("isValidHookAddress".to_owned()));
    }
}
