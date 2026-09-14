use super::*;
pub(super) fn encode(path: &[String]) -> String {
    path.iter()
        .map(|s| format!("/{}", s.replace('~', "~0").replace('/', "~1")))
        .collect()
}
pub(super) fn decode(path: &str) -> Result<Vec<String>, String> {
    if !path.starts_with('/') {
        return Err("Claude 通用配置字段路径无效".into());
    }
    let segments = path[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    if encode(&segments) != path {
        return Err("Claude 通用配置字段路径编码无效".into());
    }
    Ok(segments)
}
pub(super) fn set(root: &mut Value, path: &[String], value: Value) -> Result<(), String> {
    let Some((last, parents)) = path.split_last() else {
        return Err("Claude 通用配置不能覆盖文档根节点".into());
    };
    let mut node = root;
    for key in parents {
        let object = node
            .as_object_mut()
            .ok_or("Claude 通用配置的父节点不是对象，请先处理类型冲突")?;
        node = object
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    node.as_object_mut()
        .ok_or("Claude 通用配置的父节点不是对象，请先处理类型冲突")?
        .insert(last.clone(), value);
    Ok(())
}
pub(super) fn remove(root: &mut Value, path: &[String]) -> Result<(), String> {
    let Some((last, parents)) = path.split_last() else {
        return Err("Claude 通用配置不能删除文档根节点".into());
    };
    let mut node = root;
    for key in parents {
        let Some(next) = node.as_object_mut().and_then(|node| node.get_mut(key)) else {
            return Ok(());
        };
        node = next;
    }
    if let Some(object) = node.as_object_mut() {
        object.remove(last);
    }
    Ok(())
}
