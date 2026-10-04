use super::*;

#[test]
fn job_display_filters_before_limit_without_changing_canonical_lifecycle() {
    let mut jobs = vec![json!({"job_id":"routine", "status":"completed"}); 12];
    jobs.extend((0..10).map(|index| {
        json!({"job_id":format!("active-{index}"), "status":"running",
        "terminal":true,"active":false,"stdout_tail":"PRIVATE"})
    }));
    let input = json!({"jobs":jobs,"count":22,"matched_count":22,"truncated":false});
    let before = input.clone();
    let projection = (LIST.project)("list_jobs", &input).unwrap();
    assert_eq!(input, before);
    assert_eq!(projection["routine_omitted_count"], 12);
    assert_eq!(projection["items_truncated"], true);
    assert_eq!(projection["items"].as_array().unwrap().len(), 8);
    assert_eq!(projection["items"][0]["job_id"], "active-0");
    assert_eq!(projection["items"][7]["job_id"], "active-7");
    assert_eq!(projection["items"][0]["terminal"], false);
    assert_eq!(projection["shown_active_count"], 8);
    assert!(!projection.to_string().contains("PRIVATE"));
}

#[test]
fn job_display_recovery_call_requires_the_exact_readonly_fallback() {
    let allowed = json!({"follow_up_kind":"fallback_recovery","tool":"list_jobs","arguments":{}});
    let mut item = json!({"job_id":"exact","error_kind":"unknown_job","suggested_call":allowed});
    assert_eq!(
        observed_failure_presentation(&item).unwrap()["suggested_call"],
        allowed
    );
    for call in [
        json!({"follow_up_kind":"mechanically_followable","tool":"list_jobs","arguments":{}}),
        json!({"follow_up_kind":"fallback_recovery","tool":"run_process","arguments":{}}),
        json!({"follow_up_kind":"fallback_recovery","tool":"list_jobs","arguments":{"project":"other"}}),
    ] {
        item["suggested_call"] = call;
        assert!(observed_failure_presentation(&item)
            .unwrap()
            .get("suggested_call")
            .is_none());
    }
}
