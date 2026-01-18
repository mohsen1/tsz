#[cfg(test)]
mod tests {
    use super::*;

    // This test suite assumes a `search` function in the parent module.
    //
    // Expected Signature in scanner.rs:
    // pub fn search<'a>(query: &str, contents: &'a str) -> Vec<&'a str>
    // pub fn search_case_insensitive<'a>(query: &str, contents: &'a str) -> Vec<&'a str>

    #[test]
    fn one_result() {
        let query = "duct";
        let contents = "\
Rust:
safe, fast, productive.
Pick three.";

        let result = crate::scanner::search(query, contents);

        assert_eq!(vec!["safe, fast, productive."], result);
    }

    #[test]
    fn case_insensitive() {
        let query = "rUsT";
        let contents = "\
Rust:
safe, fast, productive.
Pick three.
Trust me.";

        let result = crate::scanner::search_case_insensitive(query, contents);

        assert_eq!(vec!["Rust:", "Trust me."], result);
    }

    #[test]
    fn no_results() {
        let query = "monomorphization";
        let contents = "\
Rust:
safe, fast, productive.
Pick three.";

        let result = crate::scanner::search(query, contents);

        assert!(result.is_empty());
        assert_eq!(vec![] as Vec<&str>, result);
    }
}
```
