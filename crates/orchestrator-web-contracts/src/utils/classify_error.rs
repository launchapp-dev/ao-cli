pub fn classify_error(message: &str) -> (&'static str, i32) {
    protocol::classify_error_message(message)
}

#[cfg(test)]
mod tests {
    use super::classify_error;

    #[test]
    fn classify_error_covers_cli_parity_patterns() {
        assert_eq!(
            classify_error("required arguments were not provided: --id <ID>"),
            ("invalid_input", 2)
        );
        assert_eq!(
            classify_error("unknown argument '--bogus' found"),
            ("invalid_input", 2)
        );
        assert_eq!(
            classify_error("No such file or directory (os error 2)"),
            ("not_found", 3)
        );
        assert_eq!(classify_error("record does not exist"), ("not_found", 3));
        assert_eq!(
            classify_error("timeout while waiting for daemon"),
            ("unavailable", 5)
        );
    }
}
