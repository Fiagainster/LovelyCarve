//! 插件任务调度全生命周期。
//!
//! | 职责 | 模块 |
//! |---|---|
//! | 案件台账的 `task` / `task_followup` 表 | `store.rs` / `store_evidence.rs` |
//! | 状态机 created/queued/running/success/failed/cancel/pause/offline | `model.rs` |
//! | 从插件清单注册可调度功能 | `functions.rs` |
//! | 调度循环与任务派发 | `scheduler.rs` |
//! | 插件执行、提交结果与进度广播 | `lifecycle.rs` + `executor.rs` |
//! | 取消、暂停、启动、重启与删除任务 | `control.rs` |
//! | 离线/超时巡检与分流 | `watchdog.rs` + `lifecycle.rs::handle_offline` |
//! | 依赖链与检材上线首任务 | `dependence.rs` |
//! | `PostActionWatcher` | `post_action.rs` |
//! | `LOVELYCARVE_TASK_*` 环境变量 | `config.rs` |
//!
//! worker 是为每次执行单独拉起的插件实例(默认是
//! lc-plugin-host 子进程);同一插件的多条任务并行,
//! 总数受 `LOVELYCARVE_TASK_MAX_RUNNING` 限制;可调度 function 只有文件/检材
//! 分析能力(文件系统能力走懒探测);暂停后恢复从头重跑(LPP 暂无协作式暂停)。

mod config;
mod control;
mod dependence;
mod executor;
mod functions;
mod lifecycle;
mod model;
mod post_action;
mod scheduler;
mod store;
mod store_evidence;
mod watchdog;

#[cfg(test)]
mod scheduler_tests;

pub use config::{SchedError, SchedResult, SchedulerConfig};
pub use control::EvidenceWithdrawal;
pub use dependence::TaskTarget;
pub use executor::{BoxFuture, ExecutionOutcome, TaskExecution, TaskProgress, TaskRunner};
pub use functions::{function_name, FunctionRegistry, SchedFunction};
pub use model::{
    EvidenceTaskStatus, NewTask, SchedEvent, SchedNotice, SchedNoticeKind, SchedTaskRecord,
    SchedTaskStatus, TaskInfo, TaskParams, TaskResult,
};
pub use post_action::{PostAction, PostActionHandler, PostActionRequest};
pub use scheduler::{RegistrySource, RunnerFactory, TaskScheduler};
pub use store::{InsertOutcome, TaskFilter, TaskStore};

pub const SCHED_TASK_EVENT: &str = "sched-task-event";
