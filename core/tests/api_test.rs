use std::cell::Cell;
use spfy_core::api::retry_once_on_auth;

#[test]
fn retry_succeeds_on_second_attempt() {
    let attempts = Cell::new(0);
    let result = retry_once_on_auth(|| {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            Err(spfy_core::error::CoreError::Auth("expired".into()))
        } else {
            Ok::<_, spfy_core::error::CoreError>(42)
        }
    });
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn retry_propagates_non_auth_error_immediately() {
    let attempts = Cell::new(0);
    let result: Result<i32, _> = retry_once_on_auth(|| {
        attempts.set(attempts.get() + 1);
        Err(spfy_core::error::CoreError::Api("500".into()))
    });
    assert!(result.is_err());
    assert_eq!(attempts.get(), 1);
}
