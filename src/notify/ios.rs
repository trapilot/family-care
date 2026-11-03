use std::sync::OnceLock;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread};
use objc2_foundation::{NSArray, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNTimeIntervalNotificationTrigger, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::{ActionTitles, Category, ScheduledNotification};

const CATEGORY_REMINDER: &str = "reminder";
const CATEGORY_BILL: &str = "bill";
const ACTION_DONE: &str = "done";

/// Called with the notification id when the user taps "Done for today" / "Paid this month"
static ON_DONE: OnceLock<fn(&str)> = OnceLock::new();
/// The center keeps only a weak reference to its delegate, so we keep it alive here
static DELEGATE: OnceLock<usize> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "FamilyMoneyNotificationDelegate"]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        /// Show banners even while the app is open
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &DynBlock<dyn Fn()>,
        ) {
            if response.actionIdentifier().to_string() == ACTION_DONE {
                let id = response.notification().request().identifier().to_string();
                if let Some(on_done) = ON_DONE.get() {
                    on_done(&id);
                }
            }
            completion_handler.call(());
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

pub fn init(on_done: fn(&str)) {
    let _ = ON_DONE.set(on_done);
    DELEGATE.get_or_init(|| {
        let delegate = NotificationDelegate::new();
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Retained::into_raw(delegate) as usize
    });
}

pub fn request_permission() {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let done = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
        &done,
    );
}

/// A category whose one button (long-press / pull down the notification) sends ACTION_DONE
fn category(id: &str, button_title: &str) -> Retained<UNNotificationCategory> {
    let done = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(ACTION_DONE),
        &NSString::from_str(button_title),
        UNNotificationActionOptions::empty(),
    );
    UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(id),
        &NSArray::from_retained_slice(&[done]),
        &NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    )
}

pub fn replace_all(items: &[ScheduledNotification], now: i64, titles: &ActionTitles) {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.setNotificationCategories(&NSSet::from_retained_slice(&[
        category(CATEGORY_REMINDER, titles.done_for_today),
        category(CATEGORY_BILL, titles.paid_this_month),
    ]));

    center.removeAllPendingNotificationRequests();
    for item in items {
        let secs = (item.fire_at - now) as f64;
        if secs < 1.0 {
            continue;
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&item.title));
        content.setBody(&NSString::from_str(&item.body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        content.setThreadIdentifier(&NSString::from_str("reminders"));
        content.setCategoryIdentifier(&NSString::from_str(match item.category {
            Category::Reminder => CATEGORY_REMINDER,
            Category::Bill     => CATEGORY_BILL,
        }));

        let trigger = UNTimeIntervalNotificationTrigger::triggerWithTimeInterval_repeats(secs, false);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&item.id),
            &content,
            Some(&trigger),
        );
        center.addNotificationRequest_withCompletionHandler(&request, None);
    }
}

/// Remove already-delivered notifications (from Notification Center) by id
pub fn clear_delivered(ids: &[String]) {
    let ids: Vec<Retained<NSString>> = ids.iter().map(|id| NSString::from_str(id)).collect();
    UNUserNotificationCenter::currentNotificationCenter()
        .removeDeliveredNotificationsWithIdentifiers(&NSArray::from_retained_slice(&ids));
}
