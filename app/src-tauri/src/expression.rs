// 表情名称必须与现有模型中的名称一致。
pub fn validate_expression(name: &str) -> Result<(), String> {
    match name {
        "smile" | "squint" | "tears" | "teardrop" => Ok(()),
        _ => Err(format!("未知表情: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_expression;

    #[test]
    fn accepts_existing_expressions() {
        for name in ["smile", "squint", "tears", "teardrop"] {
            assert!(validate_expression(name).is_ok(), "应接受表情: {name}");
        }
    }

    #[test]
    fn rejects_unknown_expressions() {
        for name in ["unknown", "", "Smile", " smile "] {
            assert!(validate_expression(name).is_err(), "应拒绝表情: {name}");
        }
    }
}
