use serde_json::Value;

use crate::ccswitch::row::CodexCatalogSeed;
use crate::contracts::CodexReasoningLevel;

const SOURCE_CATALOG_ENTRY_KEYS: [&str; 16] = [
    "model",
    "displayName",
    "display_name",
    "description",
    "contextWindow",
    "context_window",
    "supportsParallelToolCalls",
    "supports_parallel_tool_calls",
    "inputModalities",
    "input_modalities",
    "baseInstructions",
    "base_instructions",
    "reasoningLevels",
    "reasoning_levels",
    "defaultReasoningLevel",
    "default_reasoning_level",
];

/// Turns the real source model catalog into editor seeds. Only facts the
/// source actually states are carried; everything else keeps `None` for the
/// editor defaults. The TOML default model is always present exactly once.
pub(super) fn catalog_seeds(
    source: Option<&Value>,
    default_model: &str,
    warnings: &mut Vec<String>,
) -> Vec<CodexCatalogSeed> {
    let mut seeds = Vec::new();
    if let Some(source) = source {
        let models = source.get("models").and_then(Value::as_array);
        match models {
            Some(models) => {
                for (index, entry) in models.iter().enumerate() {
                    if let Some(seed) = catalog_seed(entry, index, &mut seeds, warnings) {
                        seeds.push(seed);
                    }
                }
            }
            None => warnings.push("未导入: modelCatalog.models".to_string()),
        }
    }
    if !seeds.iter().any(|seed| seed.model == default_model) {
        seeds.push(CodexCatalogSeed {
            model: default_model.to_string(),
            context_window: None,
            images: None,
            display_name: None,
            description: None,
            base_instructions: None,
            supports_parallel_tool_calls: None,
            default_reasoning_level: None,
            reasoning_levels: None,
        });
    }
    seeds
}

fn catalog_seed(
    entry: &Value,
    index: usize,
    seeds: &[CodexCatalogSeed],
    warnings: &mut Vec<String>,
) -> Option<CodexCatalogSeed> {
    let object = entry.as_object().or_else(|| {
        warnings.push(format!("未导入: modelCatalog.models[{index}]"));
        None
    })?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let model = model.or_else(|| {
        warnings.push(format!("未导入: modelCatalog.models[{index}].model"));
        None
    })?;
    if seeds.iter().any(|seed| seed.model == model) {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}]（{model} 与先前条目重复）"
        ));
        return None;
    }
    let context_window = positive_u64_field(
        object
            .get("contextWindow")
            .or_else(|| object.get("context_window")),
        index,
        "contextWindow",
        warnings,
    );
    let images = images_field(
        object
            .get("inputModalities")
            .or_else(|| object.get("input_modalities")),
        index,
        warnings,
    );
    let display_name = optional_string_field(
        object
            .get("displayName")
            .or_else(|| object.get("display_name")),
        index,
        "displayName",
        warnings,
    );
    let description =
        optional_string_field(object.get("description"), index, "description", warnings);
    let base_instructions = optional_string_field(
        object
            .get("baseInstructions")
            .or_else(|| object.get("base_instructions")),
        index,
        "baseInstructions",
        warnings,
    );
    let supports_parallel_tool_calls = optional_bool_field(
        object
            .get("supportsParallelToolCalls")
            .or_else(|| object.get("supports_parallel_tool_calls")),
        index,
        "supportsParallelToolCalls",
        warnings,
    );
    let default_reasoning_level = reasoning_level_field(
        object.get("defaultReasoningLevel"),
        index,
        "defaultReasoningLevel",
        warnings,
    );
    let reasoning_levels = reasoning_levels_field(object.get("reasoningLevels"), index, warnings);
    let mut default_reasoning_level = default_reasoning_level;
    if let (Some(default), Some(levels)) = (&default_reasoning_level, &reasoning_levels) {
        if !levels.contains(default) {
            warnings.push(format!(
                "未导入: modelCatalog.models[{index}].defaultReasoningLevel 不在 reasoningLevels 内"
            ));
            default_reasoning_level = None;
        }
    }
    for key in object.keys() {
        if !SOURCE_CATALOG_ENTRY_KEYS.contains(&key.as_str()) {
            warnings.push(format!("未导入: modelCatalog.models[{index}].{key}"));
        }
    }
    Some(CodexCatalogSeed {
        model: model.to_string(),
        context_window,
        images,
        display_name,
        description,
        base_instructions,
        supports_parallel_tool_calls,
        default_reasoning_level,
        reasoning_levels,
    })
}

fn optional_string_field(
    value: Option<&Value>,
    index: usize,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let Some(value) = value else {
        return None;
    };
    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value.to_string()),
        None => {
            warnings.push(format!(
                "未导入: modelCatalog.models[{index}].{field} 必须是非空字符串"
            ));
            None
        }
    }
}

fn optional_bool_field(
    value: Option<&Value>,
    index: usize,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<bool> {
    let Some(value) = value else {
        return None;
    };
    match value.as_bool() {
        Some(value) => Some(value),
        None => {
            warnings.push(format!(
                "未导入: modelCatalog.models[{index}].{field} 必须是布尔值"
            ));
            None
        }
    }
}

fn positive_u64_field(
    value: Option<&Value>,
    index: usize,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<u64> {
    let value = value?;
    let parsed = match value {
        Value::Number(number) => number.as_u64().filter(|parsed| *parsed > 0),
        Value::String(text) => text.trim().parse::<u64>().ok().filter(|parsed| *parsed > 0),
        _ => None,
    };
    if parsed.is_none() {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}].{field} 必须是正整数"
        ));
    }
    parsed
}

fn images_field(value: Option<&Value>, index: usize, warnings: &mut Vec<String>) -> Option<bool> {
    let value = value?;
    let Some(modalities) = value.as_array() else {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}].inputModalities 必须是数组"
        ));
        return None;
    };
    let mut images = false;
    if modalities.is_empty() {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}].inputModalities 不能为空"
        ));
        return None;
    }
    for modality in modalities {
        match modality.as_str() {
            Some("text") => {}
            Some("image") => images = true,
            _ => {
                warnings.push(format!(
                    "未导入: modelCatalog.models[{index}].inputModalities 仅支持 text 或 image"
                ));
                return None;
            }
        }
    }
    Some(images)
}

fn reasoning_level_field(
    value: Option<&Value>,
    index: usize,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<CodexReasoningLevel> {
    let value = value?;
    match serde_json::from_value::<CodexReasoningLevel>(value.clone()) {
        Ok(level) => Some(level),
        Err(_) => {
            warnings.push(format!(
                "未导入: modelCatalog.models[{index}].{field} 无法识别"
            ));
            None
        }
    }
}

fn reasoning_levels_field(
    value: Option<&Value>,
    index: usize,
    warnings: &mut Vec<String>,
) -> Option<Vec<CodexReasoningLevel>> {
    let value = value?;
    let Some(list) = value.as_array() else {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}].reasoningLevels 必须是数组"
        ));
        return None;
    };
    if list.is_empty() {
        warnings.push(format!(
            "未导入: modelCatalog.models[{index}].reasoningLevels 不能为空"
        ));
        return None;
    }
    let mut levels = Vec::with_capacity(list.len());
    for item in list {
        match serde_json::from_value::<CodexReasoningLevel>(item.clone()) {
            Ok(level) => levels.push(level),
            Err(_) => {
                warnings.push(format!(
                    "未导入: modelCatalog.models[{index}].reasoningLevels 无法识别"
                ));
                return None;
            }
        }
    }
    Some(levels)
}
