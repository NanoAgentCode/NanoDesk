//! The CLI and desktop use the same task keywords and smart strategy mapping.
use super::*;
use std::collections::BTreeMap;

pub(super) fn task(content: &str, images: bool) -> String {
    if images { return "vision".into(); }
    let rules: serde_json::Value = serde_json::from_str(include_str!("../../../src/config/modelRoutingRules.json")).expect("bundled routing rules");
    let lower = content.to_lowercase();
    for entry in rules["tasks"].as_array().unwrap() {
        if entry[1].as_array().unwrap().iter().any(|word| lower.contains(word.as_str().unwrap())) {
            return entry[0].as_str().unwrap().into();
        }
    }
    "general".into()
}

pub(super) fn select(models: &[ModelConfig], fallback: &ModelConfig, content: &str, mode: &str, assignments: &BTreeMap<String,Option<String>>) -> ModelConfig {
    if mode == "manual" { return fallback.clone(); }
    let rules: serde_json::Value = serde_json::from_str(include_str!("../../../src/config/modelRoutingRules.json")).expect("bundled routing rules");
    let task = task(content,false);
    let preferred = if mode == "smart" { rules["smart"][&task].as_str().unwrap() } else { mode };
    let mut strategies = vec![preferred.to_string()];
    if mode == "smart" {
        for strategy in rules["strategies"].as_array().unwrap() {
            let strategy = strategy.as_str().unwrap();
            if strategy != preferred { strategies.push(strategy.into()); }
        }
    }
    for strategy in strategies {
        let id = assignments.get(&strategy).and_then(|id|id.as_ref());
        let candidate = if assignments.is_empty() {
            models.iter().find(|model|model.routing_enabled)
        } else { id.and_then(|id| models.iter().find(|model| &model.id == id)) };
        if let Some(model) = candidate {
            if model.routing_tasks.is_empty() || model.routing_tasks.contains(&task) { return model.clone(); }
        }
    }
    fallback.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_classification_matches_desktop_priority_and_english_variants() {
        for (text,expected) in [("翻译这段 python 代码","coding"),("please translate","translation"),("summarise this","summary"),("summarize this","summary"),("写作一篇文章","writing"),("analyse the cause","reasoning"),("hello","general")] {
            assert_eq!(task(text,false),expected);
        }
        assert_eq!(task("code",true),"vision");
    }

    #[test]
    fn assigned_strategy_models_honor_task_restrictions_and_fallbacks() {
        let make = |id: &str,tasks: Vec<&str>| {
            let draft: crate::models::ModelConfigDraft = serde_json::from_value(serde_json::json!({"id":id,"name":id,"provider":"openai-compatible","base_url":"http://localhost/v1","model":id,"api_key":"","routing_tasks":tasks})).unwrap();
            ModelConfig::from_draft_for_connectivity(draft)
        };
        let fallback=make("fallback",vec![]);
        let summary=make("summary",vec!["summary"]);
        let code=make("code",vec!["coding"]);
        let models=vec![fallback.clone(),summary.clone(),code.clone()];
        let assignments=BTreeMap::from([("quality".into(),Some("summary".into())),("speed".into(),Some("code".into()))]);
        assert_eq!(select(&models,&fallback,"debug code","quality",&assignments).id,"fallback");
        assert_eq!(select(&models,&fallback,"debug code","smart",&assignments).id,"code");
        assert_eq!(select(&models,&fallback,"debug code","manual",&assignments).id,"fallback");
        assert_eq!(select(&models,&fallback,"summary","quality",&assignments).id,"summary");
        assert_eq!(select(&models,&fallback,"debug code","quality",&BTreeMap::from([("quality".into(),None)])).id,"fallback");
    }
}
