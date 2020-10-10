use crate::task::{Task, TaskId};
use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, task::Wake};
use core::{

    future::Future,
    task::{Context, Poll, Waker},
};
use crossbeam::queue::ArrayQueue;
use lazy_static::*;
use spin::Mutex;

pub struct Executor {
    task_queue: Arc<ArrayQueue<TaskId>>,
    inner: Mutex<ExecutorInner>,
}

pub struct ExecutorInner {
    tasks: BTreeMap<TaskId, Task>,
}

impl Default for ExecutorInner {
    fn default() -> Self {
        Self {
            tasks: BTreeMap::new(),
        }
    }
}

impl Executor {
    fn new() -> Self {
        Executor {
            task_queue: Arc::new(ArrayQueue::new(1024)),
            inner: Mutex::new(ExecutorInner::default())
        }
    }

    fn add_task(&self, task: Task) {
        let mut inner = self.inner.lock();
        let task_id = task.id();
        inner.tasks.insert(task_id, task);
        self.task_queue.push(task_id).unwrap();
    }

    fn run_ready_tasks(&self) {
        let mut inner = self.inner.lock();
        if let Ok(task_id) = self.task_queue.pop() {
            let mut task = inner.tasks.remove(&task_id).unwrap();
            let waker = TaskWaker::new(task_id, Arc::clone(&self.task_queue));
            drop(inner);
            let mut context = Context::from_waker(&waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {}
                Poll::Pending => {
                    let mut inner = self.inner.lock();
                    inner.tasks.insert(task_id, task);
                }
            }
        }
    }
}

#[allow(unsafe_code)]
unsafe impl Sync for Executor {}

lazy_static! {
    static ref GLOBAL_EXECUTOR: Box<Executor> = {
        let m = Executor::new();
        Box::new(m)
    };
}

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    GLOBAL_EXECUTOR.add_task(Task::new(future));
}

pub fn run() -> ! {
    loop {
        GLOBAL_EXECUTOR.run_ready_tasks();
    }
}

struct TaskWaker {
    task_id: TaskId,
    task_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {
    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            task_queue,
        }))
    }

    fn wake_task(&self) {
        self.task_queue.push(self.task_id).unwrap();
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn x() -> usize {
        32
    }

    async fn get_x() {
        let x = x().await;
    }

    #[test]
    fn foo() {
        spawn(get_x());
        run();
    }
}
