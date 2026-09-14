use super::*;

#[test]
fn pdf_and_text_documents_preserve_payload_title_context_and_tool_errors() {
    let mut input = request("model");
    input["messages"] = json!([{ "role":"user", "content":[
        {"type":"document", "title":"Report", "context":"Focus on totals", "source":{"type":"base64", "media_type":"application/pdf", "data":"JVBERi0x"}},
        {"type":"document", "title":"Notes", "source":{"type":"text", "media_type":"text/plain", "data":"Document contents"}}
    ]}]);
    for target in TARGETS {
        let converted = convert(&input, target).unwrap();
        let text = converted.to_string();
        assert!(text.contains("data:application/pdf;base64,JVBERi0x"));
        assert!(text.contains("Focus on totals"));
        assert!(text.contains("Document contents"));
    }
}

#[test]
fn deferred_tool_reference_resolves_only_a_real_current_definition() {
    let mut input = request("model");
    input["tools"] = json!([{"name":"lookup", "description":"Search", "defer_loading":true, "input_examples":[{"q":"example"}], "input_schema":{"type":"object", "properties":{"q":{"type":"string"}}}}]);
    input["messages"] = json!([{ "role":"user", "content":[{"type":"tool_result", "tool_use_id":"search_1", "content":[{"type":"tool_reference", "tool_name":"lookup"}]}]}]);
    for target in TARGETS {
        let converted = convert(&input, target).unwrap().to_string();
        assert!(converted.contains("tool_definition"));
        assert!(converted.contains("lookup"));
        assert!(converted.contains("Input examples"));
    }
    input["messages"][0]["content"][0]["content"][0]["tool_name"] = json!("missing");
    assert!(convert(&input, TARGETS[0])
        .unwrap_err()
        .0
        .contains("不存在"));
}

#[test]
fn document_url_has_a_real_responses_representation_but_no_fake_chat_representation() {
    let mut input = request("model");
    input["messages"][0]["content"] = json!([{ "type":"document", "source":{"type":"url", "url":"https://docs.example/report.pdf"}}]);
    let converted = convert(&input, UpstreamProtocol::Responses).unwrap();
    assert!(converted.to_string().contains("file_url"));
    assert!(convert(&input, UpstreamProtocol::ChatCompletions).is_err());
}
