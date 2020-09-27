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
    tasks: BTreeMap<TaskId, Task>,
    task_queue: Arc<ArrayQueue<TaskId>>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Default for Executor {
    fn default() -> Self {
        Executor::new()
    }
}

impl Executor {
    fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(ArrayQueue::new(1024)),
            waker_cache: BTreeMap::new(),
        }
    }

    fn add_task(&mut self, task: Task) {
        let task_id = task.id();
        self.tasks.insert(task_id, task);
        self.task_queue.push(task_id).unwrap();
    }

    fn run_ready_tasks(&mut self) {
        let Self {
            tasks,
            task_queue,
            waker_cache,
        } = self;

        while let Ok(task_id) = task_queue.pop() {
            let task = match tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue,
            };
            let waker = waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::new(task_id, task_queue.clone()));
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    // task done -> remove it and its cached waker
                    tasks.remove(&task_id);
                    waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
        }
    }
}

lazy_static! {
    static ref GLOBAL_EXECUTOR: Mutex<Box<Executor>> = {
        let m = Executor::default();
        Mutex::new(Box::new(m))
    };
}

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    GLOBAL_EXECUTOR.lock().add_task(Task::new(future));
}

pub fn run() {
    GLOBAL_EXECUTOR.lock().run();
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
