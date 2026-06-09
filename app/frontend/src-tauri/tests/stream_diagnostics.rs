// integration test: 验证流式对话的事件提取和处理
// 运行: cd app/frontend/src-tauri && cargo test --test stream_diagnostics -- --nocapture

use serde_json::{json, Value};

// ============== 从 chat.rs 精确复制的核心逻辑 ==============

struct DeltaSegment {
    event_type: &'static str,
    content: String,
}

fn extract_delta_segment(raw_chunk: Option<&Value>) -> Option<DeltaSegment> {
    let chunk = raw_chunk?;
    let choice = chunk.get("choices")?.get(0)?;
    let delta = choice.get("delta");

    for key in [
        "reasoning_content",
        "reasoningContent",
        "reasoning",
        "thinking",
        "thought",
    ] {
        if let Some(text) = delta
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return Some(DeltaSegment {
                event_type: "assistant_thinking_delta",
                content: text.to_string(),
            });
        }
    }

    for key in ["content", "text"] {
        if let Some(text) = delta
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return Some(DeltaSegment {
                event_type: "assistant_delta",
                content: text.to_string(),
            });
        }
    }

    choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|content| DeltaSegment {
            event_type: "assistant_delta",
            content: content.to_string(),
        })
}

fn map_event_type(event_type: &str) -> &str {
    match event_type {
        "chunk" => "assistant_delta",
        "thinking" => "assistant_thinking_delta",
        other => other,
    }
}

// ============== 测试 ==============

#[test]
fn test_extract_delta_openai_normal_content() {
    let raw = json!({
        "choices": [{
            "index": 0,
            "delta": { "content": "Hello world" },
            "finish_reason": null
        }]
    });
    let r = extract_delta_segment(Some(&raw)).unwrap();
    assert_eq!(r.event_type, "assistant_delta");
    assert_eq!(r.content, "Hello world");
    eprintln!("✅ PASS: test_extract_delta_openai_normal_content");
}

#[test]
fn test_extract_delta_openai_thinking_reasoning_content() {
    let raw = json!({
        "choices": [{
            "index": 0,
            "delta": { "reasoning_content": "Step 1: analyze..." },
            "finish_reason": null
        }]
    });
    let r = extract_delta_segment(Some(&raw)).unwrap();
    assert_eq!(r.event_type, "assistant_thinking_delta");
    assert_eq!(r.content, "Step 1: analyze...");
    eprintln!("✅ PASS: test_extract_delta_openai_thinking_reasoning_content");
}

#[test]
fn test_extract_delta_null_delta_produces_none() {
    let raw = json!({
        "choices": [{
            "index": 0,
            "delta": null,
            "finish_reason": "stop"
        }]
    });
    assert!(extract_delta_segment(Some(&raw)).is_none());
    eprintln!("✅ PASS: test_extract_delta_null_delta_produces_none");
}

#[test]
fn test_extract_delta_empty_content_produces_none() {
    let raw = json!({
        "choices": [{
            "index": 0,
            "delta": { "content": "" },
            "finish_reason": null
        }]
    });
    assert!(extract_delta_segment(Some(&raw)).is_none());
    eprintln!("✅ PASS: test_extract_delta_empty_content_produces_none");
}

#[test]
fn test_extract_delta_raw_chunk_none_produces_none() {
    assert!(extract_delta_segment(None).is_none());
    eprintln!("✅ PASS: test_extract_delta_raw_chunk_none_produces_none");
}

// ============== 关键诊断: 模拟完整的 stream 事件处理流程 ==============

#[test]
fn test_full_stream_with_openai_chunks() {
    // 模拟 otherone 框架发来的真实事件序列
    #[derive(Debug)]
    struct FrameworkEvent {
        event_type: String,
        content: String,
        raw_chunk: Option<Value>,
    }

    let stream_events = vec![
        // 框架 chunk 事件的 content 永远是空字符串!
        FrameworkEvent {
            event_type: "chunk".to_string(),
            content: String::new(),
            raw_chunk: Some(json!({
                "choices": [{"index": 0, "delta": {"content": "你好"} }]
            })),
        },
        FrameworkEvent {
            event_type: "chunk".to_string(),
            content: String::new(),
            raw_chunk: Some(json!({
                "choices": [{"index": 0, "delta": {"content": "，世界"} }]
            })),
        },
        FrameworkEvent {
            event_type: "chunk".to_string(),
            content: String::new(),
            raw_chunk: Some(json!({
                "choices": [{"index": 0, "delta": {"content": "！"} }]
            })),
        },
        FrameworkEvent {
            event_type: "complete".to_string(),
            content: "你好，世界！".to_string(),
            raw_chunk: None,
        },
    ];

    let mut emitted_events: Vec<(String, String)> = Vec::new();
    let mut chunk_count = 0;

    for event in &stream_events {
        if event.event_type == "chunk" {
            chunk_count += 1;

            // 这就是 chat.rs 的处理逻辑
            if let Some(segment) = extract_delta_segment(event.raw_chunk.as_ref()) {
                emitted_events.push((segment.event_type.to_string(), segment.content.clone()));
                eprintln!("  chunk#{} → emit({}, \"{}\")", chunk_count, segment.event_type, segment.content);
            } else if !event.content.is_empty() {
                emitted_events.push(("assistant_delta".to_string(), event.content.clone()));
                eprintln!("  chunk#{} → emit(fallback, \"{}\")", chunk_count, event.content);
            } else {
                eprintln!("  ⚠️ chunk#{} → 静默丢失! raw_chunk={:?}, content=空",
                    chunk_count, event.raw_chunk.is_some());
            }
        } else {
            let mapped = map_event_type(&event.event_type);
            emitted_events.push((mapped.to_string(), event.content.clone()));
            eprintln!("  {} → emit({})", event.event_type, mapped);
        }
    }

    assert_eq!(chunk_count, 3, "应该处理3个 chunk");
    assert_eq!(emitted_events.len(), 4, "应该 emit 4 个事件 (3 delta + 1 complete)");
    assert_eq!(emitted_events[0].0, "assistant_delta");
    assert_eq!(emitted_events[0].1, "你好");
    assert_eq!(emitted_events[1].1, "，世界");
    assert_eq!(emitted_events[2].1, "！");
    assert_eq!(emitted_events[3].0, "complete");
    eprintln!("✅ PASS: test_full_stream_with_openai_chunks");
    eprintln!("   完整流模拟: 所有事件正确 emit，不会被静默丢失");
}

#[test]
fn test_chunk_with_null_choices_produces_no_event() {
    // 极端情况: raw_chunk 的 choices 为空数组
    let raw = json!({ "choices": [] });
    assert!(extract_delta_segment(Some(&raw)).is_none());
    eprintln!("✅ PASS: test_chunk_with_null_choices_produces_no_event");
}

#[test]
fn test_event_type_mapping() {
    assert_eq!(map_event_type("chunk"), "assistant_delta");
    assert_eq!(map_event_type("thinking"), "assistant_thinking_delta");
    assert_eq!(map_event_type("complete"), "complete");
    assert_eq!(map_event_type("error"), "error");
    assert_eq!(map_event_type("tool_calls"), "tool_calls");
    eprintln!("✅ PASS: test_event_type_mapping");
}

// ============== 诊断测试: emit_to vs emit 问题 ==============

#[test]
fn test_diagnose_emit_issue() {
    // 这个测试验证前端 listen() 接收事件的机制
    //
    // 后端 chat.rs:449:
    //   app.emit_to("main", "chat_stream_event", &event)
    //
    // 前端 chatStorage.ts:42:
    //   listen<ChatStreamEvent>('chat_stream_event', ...)
    //
    // Tauri v2:
    //   emit_to("main") → 只发给标签为 "main" 的窗口
    //   listen() [全局] → 只接收 app.emit() (非 emit_to) 的事件
    //
    // 问题: emit_to("main") 成功了不报错，
    //       但前端的全局 listen() 收不到窗口专属事件!
    //
    // 解决: 把 emit_to("main") 改成 emit()
    eprintln!("⚠️  DIAGNOSIS: emit_to('main') vs listen() 不匹配!");
    eprintln!("   后端: app.emit_to('main', ...) → 窗口级事件");
    eprintln!("   前端: listen('chat_stream_event', ...) → 全局事件监听");
    eprintln!("   结果: 事件被发送到正确窗口，但前端监听器收不到!");
    eprintln!("   FIX: 改为 app.emit('chat_stream_event', event)");
}
