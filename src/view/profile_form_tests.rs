use super::subscription_name_valid;
use super::{Message, subscription_form_status};

#[test]
fn subscription_name_accepts_blank_and_counts_unicode_characters() {
    assert!(subscription_name_valid(""));
    assert!(subscription_name_valid("   "));
    assert!(subscription_name_valid(&"a".repeat(80)));
    assert!(!subscription_name_valid(&"a".repeat(81)));
    // The limit is in characters rather than UTF-8 bytes.
    assert!(subscription_name_valid(&"猫".repeat(80)));
    assert!(!subscription_name_valid(&"猫".repeat(81)));
}

#[test]
fn form_status_shows_pending_then_error_and_clears_on_retry() {
    let pending = Message::SavingSubscriptionName;
    let failure = Message::SubscriptionNameSaveFailed;
    assert_eq!(subscription_form_status(false, pending, None), None);
    assert_eq!(subscription_form_status(true, pending, None), Some(pending));
    assert_eq!(
        subscription_form_status(false, pending, Some(failure)),
        Some(failure)
    );
    assert_eq!(subscription_form_status(true, pending, None), Some(pending));
    assert_eq!(subscription_form_status(false, pending, None), None);
}
