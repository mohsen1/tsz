use crate::transforms::LirNode;
use crate::transforms::Literal;

/// Configuration for the emitter, allowing for formatting preferences.
#[derive(Clone, Copy, Debug)]
pub struct EmitConfig {
    pub indent_size: usize,
    pub newline: &'static str,
}

impl Default for EmitConfig {
    fn default() -> Self {
        Self {
            indent_size: 4,
            newline: "\n",
        }
    }
}

/// The main entry point to convert an LIR node into a C code string.
pub fn emit_node(node: &LirNode) -> String {
    let config = EmitConfig::default();
    let mut buffer = String::new();
    emit_node_inner(node, &config, 0, &mut buffer);
    buffer
}

/// Recursive helper to build the string with indentation.
fn emit_node_inner(node: &LirNode, config: &EmitConfig, indent: usize, buffer: &mut String) {
    match node {
        LirNode::Module(children) => {
            for child in children {
                emit_node_inner(child, config, indent, buffer);
                match child {
                    LirNode::Function { .. } | LirNode::StructDef { .. } => {
                        buffer.push_str(config.newline);
                    }
                    _ => {}
                }
            }
        }

        LirNode::StructDef { name, fields } => {
            emit_indent(buffer, config, indent);
            buffer.push_str(&format!("struct {} {{", name));
            buffer.push_str(config.newline);
            
            for (ftype, fname) in fields {
                emit_indent(buffer, config, indent + 1);
                buffer.push_str(&format!("{} {};", ftype, fname));
                buffer.push_str(config.newline);
            }
            
            emit_indent(buffer, config, indent);
            buffer.push_str("};");
            buffer.push_str(config.newline);
        }

        LirNode::Function { return_type, name, args, body } => {
            emit_indent(buffer, config, indent);
            let args_str = args.iter()
                .map(|(t, n)| format!("{} {}", t, n))
                .collect::<Vec<_>>()
                .join(", ");
            
            buffer.push_str(&format!("{} {}({}) ", return_type, name, args_str));
            emit_node_inner(body, config, indent, buffer); // Block handles its own brackets
            buffer.push_str(config.newline);
        }

        LirNode::Block(stmts) => {
            buffer.push_str("{");
            buffer.push_str(config.newline);
            for stmt in stmts {
                emit_node_inner(stmt, config, indent + 1, buffer);
            }
            emit_indent(buffer, config, indent);
            buffer.push_str("}");
        }

        LirNode::VarDecl { vtype, name, init } => {
            emit_indent(buffer, config, indent);
            buffer.push_str(&format!("{} {}", vtype, name));
            if let Some(init_val) = init {
                buffer.push_str(" = ");
                // Don't add newline after the expression if it's part of an init
                emit_expr(init_val, buffer); 
            }
            buffer.push_str(";");
            buffer.push_str(config.newline);
        }

        LirNode::Assignment { lhs, rhs } => {
            emit_indent(buffer, config, indent);
            emit_expr(lhs, buffer);
            buffer.push_str(" = ");
            emit_expr(rhs, buffer);
            buffer.push_str(";");
            buffer.push_str(config.newline);
        }

        LirNode::Call { func, args } => {
            emit_indent(buffer, config, indent);
            buffer.push_str(&format!("{}(", func));
            for (i, arg) in args.iter().enumerate() {
                if i > 0 { buffer.push_str(", "); }
                emit_expr(arg, buffer);
            }
            buffer.push_str(");");
            buffer.push_str(config.newline);
        }

        LirNode::If { condition, then_branch, else_branch } => {
            emit_indent(buffer, config, indent);
            buffer.push_str("if (");
            emit_expr(condition, buffer);
            buffer.push_str(") ");
            emit_node_inner(then_branch, config, indent, buffer);
            
            if let Some(else_b) = else_branch {
                buffer.push_str(" else ");
                // If the else branch is a block, emit inline. If it's another if, emit inline.
                // Otherwise, just emit.
                match else_b.as_ref() {
                    LirNode::Block(_) | LirNode::If { .. } => {
                        emit_node_inner(else_b, config, indent, buffer);
                    }
                    _ => {
                        buffer.push_str("{"); // Wrap single statement else in block for safety
                        buffer.push_str(config.newline);
                        emit_node_inner(else_b, config, indent + 1, buffer);
                        emit_indent(buffer, config, indent);
                        buffer.push_str("}");
                    }
                }
            }
            buffer.push_str(config.newline);
        }

        LirNode::While { condition, body } => {
            emit_indent(buffer, config, indent);
            buffer.push_str("while (");
            emit_expr(condition, buffer);
            buffer.push_str(") ");
            emit_node_inner(body, config, indent, buffer);
            buffer.push_str(config.newline);
        }

        LirNode::Return(val) => {
            emit_indent(buffer, config, indent);
            buffer.push_str("return");
            if let Some(v) = val {
                buffer.push_str(" ");
                emit_expr(v, buffer);
            }
            buffer.push_str(";");
            buffer.push_str(config.newline);
        }

        LirNode::Comment(text) => {
            emit_indent(buffer, config, indent);
            buffer.push_str(&format!("// {}", text));
            buffer.push_str(config.newline);
        }

        // Expressions typically don't trigger their own newlines/indents unless they are statements
        _
