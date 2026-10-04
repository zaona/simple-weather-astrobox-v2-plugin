//! 基于宿主定时器的异步等待。
//!
//! `std::thread::sleep` 会卡住整个插件实例，期间其它 UI 事件都只能排队。这里改为
//! `timer::set_timeout` 登记一个定时器，到期时宿主派发的 timer 事件由 `on_event`
//! 交给 [`handle_timer_payload`] 唤醒对应的等待者。跨组件任务唤醒依赖 wit-bindgen 的
//! `inter-task-wakeup` 特性。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::astrobox::psys_host_v4::timer;

const PAYLOAD_PREFIX: &str = "__plugin_sleep:";

#[derive(Default)]
struct Slot {
    fired: bool,
    waker: Option<Waker>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SLOTS: Mutex<Option<HashMap<u64, Slot>>> = Mutex::new(None);

fn with_slots<R>(f: impl FnOnce(&mut HashMap<u64, Slot>) -> R) -> R {
    let mut guard = SLOTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    with_slots(|slots| slots.insert(id, Slot::default()));
    let delay_ms = duration.as_millis().max(1) as u64;
    let timer_id = timer::set_timeout(delay_ms, &format!("{PAYLOAD_PREFIX}{id}"));
    Sleep { id, timer_id }
}

/// 处理 timer 事件里的 payload 字段；属于 [`sleep`] 的返回 `true`。
pub fn handle_timer_payload(payload: &str) -> bool {
    let Some(id) = payload
        .strip_prefix(PAYLOAD_PREFIX)
        .and_then(|rest| rest.parse::<u64>().ok())
    else {
        return false;
    };

    let waker = with_slots(|slots| {
        slots.get_mut(&id).and_then(|slot| {
            slot.fired = true;
            slot.waker.take()
        })
    });
    if let Some(waker) = waker {
        waker.wake();
    }
    true
}

struct Sleep {
    id: u64,
    timer_id: u64,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let id = self.id;
        with_slots(|slots| match slots.get_mut(&id) {
            Some(slot) if !slot.fired => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
            _ => Poll::Ready(()),
        })
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        let fired = with_slots(|slots| slots.remove(&self.id).is_some_and(|slot| slot.fired));
        if !fired {
            timer::clear_timer(self.timer_id);
        }
    }
}
