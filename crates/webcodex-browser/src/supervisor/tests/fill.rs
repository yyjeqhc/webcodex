use super::*;

#[test]
#[ignore = "requires local Chromium; synthetic data only, isolated owned profile"]
fn chromium_batch_replaces_32_native_text_fields_with_one_query_and_readback() {
    let supervisor = BrowserSupervisor::new();
    let _shutdown = ShutdownOnDrop(&supervisor);
    let mut html = String::from("<!doctype html><meta charset=utf-8><title>Batch fixture</title>");
    for i in 0..32 {
        if i % 2 == 0 {
            html.push_str(&format!("<input aria-label='Field {i}' value='old value'>"));
        } else {
            html.push_str(&format!(
                "<textarea aria-label='Field {i}'>old value</textarea>"
            ));
        }
    }
    // A framework-like tracker must observe replacement through the native setter.
    html.push_str(
        r#"<script>
      for (const el of document.querySelectorAll('input,textarea')) {
        const native = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
        let tracked = el.value;
        Object.defineProperty(el, 'value', {
          get() { return native.get.call(this); },
          set(v) { tracked = v; native.set.call(this, v); }
        });
        el.addEventListener('input', () => {
          if (tracked === el.value) native.set.call(el, 'tracker missed update');
          tracked = el.value;
        });
      }
    </script>"#,
    );
    let form = FormPage::serve_html(html);
    let browser = supervisor.launch().unwrap();
    let page = supervisor.pages(&browser.browser_id, 1).unwrap().remove(0);
    supervisor
        .navigate(
            &browser.browser_id,
            &page.page_id,
            &format!("http://127.0.0.1:{}/", form.port),
        )
        .unwrap();
    let query = crate::BrowserSnapshotQuery {
        fields_only: true,
        ..Default::default()
    };
    let snapshot = supervisor
        .snapshot_query(
            &browser.browser_id,
            &page.page_id,
            SnapshotMode::Auto,
            256,
            32,
            0,
            Some(&query),
        )
        .unwrap();
    assert!(!snapshot.truncated);
    assert_eq!(snapshot.nodes.len(), 32);
    let operations = snapshot
        .nodes
        .iter()
        .map(|node| {
            assert!(node.actions.iter().any(|a| a == "set_value"));
            BatchOperation::SetValue {
                element_id: node.element_id.clone().unwrap(),
                value: "fictional replacement".into(),
            }
        })
        .collect::<Vec<_>>();
    let receipt = supervisor
        .batch(&browser.browser_id, &page.page_id, &operations)
        .unwrap();
    assert_eq!(receipt.completed_count, 32);
    assert_eq!(receipt.remaining_count, 0);
    assert!(receipt.error.is_none(), "{:?}", receipt.error);
    let readback = supervisor
        .snapshot_query(
            &browser.browser_id,
            &page.page_id,
            SnapshotMode::Auto,
            256,
            32,
            0,
            Some(&query),
        )
        .unwrap();
    assert_eq!(readback.nodes.len(), 32);
    assert!(readback
        .nodes
        .iter()
        .all(|node| node.value.as_deref() == Some("fictional replacement")));
    let stopped = supervisor
        .batch(&browser.browser_id, &page.page_id, &operations)
        .unwrap();
    assert_eq!(stopped.completed_count, 0);
    assert_eq!(stopped.remaining_count, 32);
}
