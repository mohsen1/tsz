/let interface_idx = symbol/ {
    print "                    let mut all_interface_members = Vec::new();"
    print "                    let mut interface_type_params = None;"
    print "                    let mut has_private_members = false;"
    print "                    let mut is_class = false;"
    print "                    let mut is_interface = false;"
    print "                    for &decl_idx in &symbol.declarations {"
    print "                        if let Some(node) = self.ctx.arena.get(decl_idx) {"
    print "                            if node.kind == syntax_kind_ext::CLASS_DECLARATION {"
    print "                                is_class = true;"
    print "                                if let Some(base_class_data) = self.ctx.arena.get_class(node) {"
    print "                                    if self.class_has_private_or_protected_members(base_class_data) {"
    print "                                        has_private_members = true;"
    print "                                    }"
    print "                                    all_interface_members.extend(&base_class_data.members.nodes);"
    print "                                    if interface_type_params.is_none() {"
    print "                                        interface_type_params = base_class_data.type_parameters.clone();"
    print "                                    }"
    print "                                }"
    print "                            } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {"
    print "                                is_interface = true;"
    print "                                if let Some(interface_decl) = self.ctx.arena.get_interface(node) {"
    print "                                    if self.interface_extends_class_with_inaccessible_members("
    print "                                        decl_idx, interface_decl, class_idx, class_data,"
    print "                                    ) {"
    print "                                        self.error_at_node("
    print "                                            type_idx,"
    print "                                            &format!(\"Class '{class_name}' incorrectly implements interface '{interface_name}'.\"),"
    print "                                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,"
    print "                                        );"
    print "                                        return; // or continue outer loop somehow"
    print "                                    }"
    print "                                    all_interface_members.extend(&interface_decl.members.nodes);"
    print "                                    if interface_type_params.is_none() {"
    print "                                        interface_type_params = interface_decl.type_parameters.clone();"
    print "                                    }"
    print "                                }"
    print "                            }"
    print "                        }"
    print "                    }"
    print "                    if has_private_members {"
    print "                        let message = format!(\"Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?\");"
    print "                        self.error_at_node(type_idx, &message, diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER);"
    print "                        continue;"
    print "                    }"
    print "                    if !is_class && !is_interface {"
    print "                        continue;"
    print "                    }"
    # now skip all the old code up to missing_members
    skip = 1
}
/let mut missing_members: Vec<String> = Vec::new();/ {
    skip = 0
    print
    next
}
/for &member_idx in &interface_members.nodes {/ {
    if (!skip) {
        print "                    for &member_idx in &all_interface_members {"
        next
    }
}
/let diagnostic_code =/ {
    print "                    let diagnostic_code ="
    print "                        if is_class {"
    print "                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER"
    print "                        } else if interface_has_index_signature {"
    print "                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE"
    print "                        } else {"
    print "                            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE"
    print "                        };"
    skip = 1
    next
}
/if !missing_members.is_empty() {/ {
    skip = 0
    print
    next
}
{
    if (!skip) print
}
