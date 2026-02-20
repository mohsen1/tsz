/if body_node.kind == syntax_kind_ext::MODULE_BLOCK {/ {
    print "        let mut is_ambient_external_module = false;"
    print "        if let Some(ext) = self.ctx.arena.get_extended(body_idx) {"
    print "            let parent_idx = ext.parent;"
    print "            if !parent_idx.is_none() {"
    print "                if let Some(parent_node) = self.ctx.arena.get(parent_idx) {"
    print "                    if let Some(module) = self.ctx.arena.get_module(parent_node) {"
    print "                        if let Some(name_node) = self.ctx.arena.get(module.name) {"
    print "                            if name_node.kind == syntax_kind_ext::STRING_LITERAL {"
    print "                                is_ambient_external_module = true;"
    print "                            }"
    print "                        }"
    print "                    }"
    print "                }"
    print "            }"
    print "        }"
    print ""
    print $0
    next
}
/if is_export_assign {/ {
    print "                    if is_export_assign && !is_ambient_external_module {"
    next
}
/Filter out export assignments in namespace bodies since they're already/ {
    print "                // Filter out export assignments in namespace bodies since they're already"
    print "                // flagged with TS1063 and shouldn't trigger TS2304/TS2309 follow-up errors."
    print "                // However, they ARE checked in ambient external modules."
    next
}
/let non_export_assign: Vec<NodeIndex> = statements/ {
    print "                let non_export_assign: Vec<NodeIndex> = if is_ambient_external_module {"
    print "                    statements.nodes.clone()"
    print "                } else {"
    print "                    statements.nodes"
    print "                        .iter()"
    print "                        .copied()"
    print "                        .filter(|&idx| {"
    print "                            self.ctx"
    print "                                .arena"
    print "                                .get(idx)"
    print "                                .is_none_or(|n| n.kind != syntax_kind_ext::EXPORT_ASSIGNMENT)"
    print "                        })"
    print "                        .collect()"
    print "                };"
    print "                self.check_export_assignment(&non_export_assign);"
    print "            }"
    print "        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {"
    exit
}
{ print }
