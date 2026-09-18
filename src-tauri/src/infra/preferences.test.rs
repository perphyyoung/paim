use super::*;

const OK: &str =
    r#"{"app":"paim","kind":"preferences","version":1,"preferences":{"fontScale":"120"}}"#;

#[test]
fn accepts_own_file() {
    assert!(validate_preference_file(OK).is_ok());
}

#[test]
fn rejects_non_json() {
    let err = validate_preference_file("not json").unwrap_err();
    assert!(err.to_string().contains("JSON"), "{err}");
}

#[test]
fn rejects_other_app_or_kind() {
    assert!(
        validate_preference_file(r#"{"app":"other","kind":"preferences","preferences":{}}"#)
            .is_err()
    );
    assert!(
        validate_preference_file(r#"{"app":"paim","kind":"backup","preferences":{}}"#).is_err()
    );
}

#[test]
fn rejects_missing_or_non_object_preferences() {
    assert!(validate_preference_file(r#"{"app":"paim","kind":"preferences"}"#).is_err());
    assert!(
        validate_preference_file(r#"{"app":"paim","kind":"preferences","preferences":"x"}"#)
            .is_err()
    );
    // 空对象的偏好文件是合法的（用户什么都没改过）
    assert!(
        validate_preference_file(r#"{"app":"paim","kind":"preferences","preferences":{}}"#).is_ok()
    );
}
