#[cfg(test)]
mod integration {
    use std::sync::Arc;
    use async_channel::Sender;
    use crate::model::{ModelInfo, ModelProvider, StreamEvent, tool_parser::ToolCallAggregator};
    use crate::protocol::context::AgentSelectionDescriptor;
    use crate::protocol::messages::SessionEntry;
    use crate::runtime::budget::AgentBudget;
    use crate::runtime::engine::{AgentService, ChatMessage, AgentConfirmResult, AgentQuestionAnswer, AgentFrontendResult};
    use crate::runtime::r#loop::AgentRuntime;
    use crate::runtime::compaction::{Compactor, estimate_tokens, messages_token_count, check_context_budget};
    use crate::runtime::doom_loop::{DoomLoopDetector, DoomLoopStatus, build_doom_signature};
    use crate::runtime::confirm::{ConfirmSession, ConfirmRequest, QuestionRequest, QuestionOption, ConfirmAnswer, QuestionAnswer};
    use crate::runtime::adapter::build_context_snapshot;
    use crate::runtime::persistence::{FsPersistenceStore, PersistenceStore};
    use crate::tools::concrete::blocks::register_native_tools;
    use crate::tools::registry::ToolRegistry;
    use crate::tools::mutation::AgentMutationIntent;
    use crate::tools::read::{AgentBlockSummary, DocumentStat};
    use crate::{AgentSessionId, BlockId, DocumentId, JsonValue, TurnId};

    fn make_context() -> crate::protocol::context::AgentContextSnapshot {
        build_context_snapshot(DocumentId::new_v4(), Some("Test"), 1, 3, None, &[], &[BlockId::new_v4()], 1000)
    }

    fn make_registry() -> Arc<ToolRegistry> {
        let mut r = ToolRegistry::new();
        register_native_tools(&mut r);
        Arc::new(r)
    }

    fn make_service() -> AgentService {
        AgentService::new(AgentSessionId::new_v4(), make_registry(), AgentBudget::new(100000, 300), crate::runtime::adapter::tests::mock_agent_ports())
    }

    struct MockProvider { responses: Vec<String> }
    impl ModelProvider for MockProvider {
        fn models(&self) -> Vec<ModelInfo> { vec![ModelInfo { id: "mock".into(), name: "Mock".into(), context_limit: 4096 }] }
        fn default_model(&self) -> &str { "mock" }
        fn context_limit(&self, _: &str) -> usize { 4096 }
        fn stream_chat(&self, _m: &str, _msgs: Vec<ChatMessage>, _tools: Vec<JsonValue>, sender: Sender<StreamEvent>) {
            for r in &self.responses { let _ = sender.send_blocking(StreamEvent::Delta(r.clone())); }
            let _ = sender.send_blocking(StreamEvent::Done { prompt_tokens: 10, completion_tokens: 5 });
        }
    }

    #[test] fn full_turn_with_mock() {
        let ctx = make_context();
        let svc = make_service();
        let p: Box<dyn ModelProvider> = Box::new(MockProvider { responses: vec!["OK".into()] });
        let mut rt = AgentRuntime::new(svc, std::sync::Arc::from(p), "mock".into());
        assert!(rt.run_turn("test", &ctx).is_ok());
    }

    #[test] fn eight_tools_registered() {
        let reg = make_registry();
        for n in &["block.get_summary","block.get_markdown","block.list_children","block.get_structured","block.replace","block.insert","block.delete","document.stat"] {
            assert!(reg.find(n).is_some(), "missing: {n}");
        }
        assert_eq!(reg.len(), 8);
    }

    #[test] fn confirm_workflow() {
        let mut s = ConfirmSession::new();
        s.request_confirm(ConfirmRequest { request_id: "i1".into(), summary: "x".into(), intent: AgentMutationIntent::DeleteBlocks { targets: vec![] }, preview: None, created_at_ms: 1 });
        s.answer_confirm(ConfirmAnswer { request_id: "i1".into(), approved: true, always: false }).unwrap();
        assert_eq!(s.pending_confirms_list().len(), 0);
    }

    #[test] fn question_workflow() {
        let mut s = ConfirmSession::new();
        s.request_question(QuestionRequest { question_id: "q1".into(), title: "Q".into(), options: vec![QuestionOption { label: "A".into(), description: None }], created_at_ms: 1 });
        s.answer_question(QuestionAnswer { question_id: "q1".into(), answers: vec!["A".into()] }).unwrap();
        assert_eq!(s.pending_questions_list().len(), 0);
    }

    #[test] fn token_estimate() { assert!(estimate_tokens("hello world") >= 2);  }

    #[test] fn compaction_noop_on_small() {
        let c = Compactor::new();
        let msgs = vec![ChatMessage { role: "system".into(), content: Some("s".into()), tool_calls: None, tool_call_id: None }];
        let (out, r) = c.compact_messages(&msgs);
        assert_eq!(r.entries_removed, 0);
    }

    #[test] fn doom_loop_reset() {
        let mut d = DoomLoopDetector::new();
        d.record_tool_call("a", 1); d.record_tool_call("a", 1);
        assert_eq!(d.record_tool_call("b", 2), DoomLoopStatus::Normal);
    }

    #[test] fn doom_signature_per_tool() {
        let s1 = build_doom_signature("block.replace", &serde_json::json!({"id":"a"}));
        let s2 = build_doom_signature("block.replace", &serde_json::json!({"id":"b"}));
        assert_ne!(s1, s2);
    }

    #[test] fn retry_classification() {
        use crate::runtime::engine::AgentService;
        assert_eq!(AgentService::classify_retry_error("401"), "unretryable");
        assert_eq!(AgentService::classify_retry_error("429"), "rate_limit");
        assert_eq!(AgentService::classify_retry_error("502"), "internal_server");
        assert_eq!(AgentService::classify_retry_error("timeout"), "timeout");
        assert_eq!(AgentService::classify_retry_error("connection"), "network");
    }

    #[test] fn persistence_roundtrip() {
        let root = std::env::temp_dir().join(format!("int-{}", uuid::Uuid::new_v4()));
        let store = FsPersistenceStore::new(root);
        let sid = AgentSessionId::new_v4();
        let e = vec![SessionEntry { id: "e1".into(), role: "user".into(), content: "hi".into(), tool_calls: vec![], references: vec![], editor_context: None, created_at_ms: 1 }];
        store.save_entries(sid, &e).unwrap();
        assert_eq!(store.load_entries(sid).unwrap().len(), 1);
        store.delete_session(sid).unwrap();
    }

    #[test] fn tool_aggregator() {
        let mut a = ToolCallAggregator::new();
        a.feed("a", Some("t"), r#"{"x":1}"#, false);
        assert_eq!(a.pending_count(), 1);
    }

    #[test] fn snapshot_defaults() {
        let s = build_context_snapshot(DocumentId::new_v4(), None, 0, 0, None, &[], &[], 1000);
        assert!(s.focused.is_none());
        assert_eq!(s.visible_window.visible_block_ids.len(), 0);
    }

    #[test] fn budget_consumes_tool_calls() {
        let mut b = AgentBudget::new(10000, 300);
        assert!(b.consume_tool_call().is_ok());
    }
}
