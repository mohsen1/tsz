use super::Server;

type PropEntry = (String, String, bool);

#[derive(Debug, Clone)]
pub(super) struct JSDocParamTag {
    pub(super) path: Vec<String>,
    pub(super) ty: String,
    pub(super) optional: bool,
    pub(super) explicit_type: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObjectParamNode {
    pub(super) ty: Option<String>,
    pub(super) optional: bool,
    pub(super) children: std::collections::BTreeMap<String, ObjectParamNode>,
}

include!("handlers_code_fixes_jsdoc_parts/part1.rs");
include!("handlers_code_fixes_jsdoc_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::Server;

    #[test]
    fn normalize_jsdoc_function_type() {
        assert_eq!(
            Server::normalize_jsdoc_type("function(*, ...number, ...boolean): void"),
            "(arg0: any, arg1: number[], ...rest: boolean[]) => void"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("function(this:{ a: string}, string, number): boolean"),
            "(this: { a: string; }, arg1: string, arg2: number) => boolean"
        );
    }

    #[test]
    fn normalize_jsdoc_object_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("Object<string, boolean>"),
            "{ [s: string]: boolean; }"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("Object<number, string>"),
            "{ [n: number]: string; }"
        );
    }

    #[test]
    fn normalize_jsdoc_promise_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("promise<String>"),
            "Promise<string>"
        );
    }

    #[test]
    fn jsdoc_fallback_object_index_signatures() {
        let src = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb, ns) {\n    sb; ns;\n}\n";
        let expected = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb: { [s: string]: boolean; }, ns: { [n: number]: string; }) {\n    sb; ns;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }

    #[test]
    fn jsdoc_fallback_template_function() {
        let src = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f(a, b) {\n    return a || b;\n}\n";
        let expected = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f<T>(a: number, b: T) {\n    return a || b;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }
}
