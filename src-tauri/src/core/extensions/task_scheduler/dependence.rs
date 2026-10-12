//! 任务依赖链与证据上线首任务。
//!
//! - 证据上线:为每个 `trigger=auto` 且无前驱的 function 建首任务;
//! - 任务成功:查询 `dependence == 该任务 function` 的下一级 function,逐个
//!   建任务(同检材、继承目标文件参数、`parent_tid` 链);function 已禁用
//!   则记日志跳过("funciton %s not exist");
//! - 任务失败/取消:不建后续任务。
//!
//! 由依赖链或证据上线创建的任务 `trigger = auto`,离线时重排队;用户手动
//! 创建的任务 `trigger = manually`,离线时判失败。

use super::{
    functions::{FunctionRegistry, SchedFunction},
    model::{NewTask, SchedTaskRecord, SchedTaskStatus, TaskParams},
    store::{InsertOutcome, TaskStore},
};
use crate::core::extensions::plugin::TaskTrigger;

/// 建任务的归属上下文:台账检材主键与目标参数。
#[derive(Clone, Debug, PartialEq)]
pub struct TaskTarget {
    pub evidence_id: i64,
    pub params: TaskParams,
}

pub fn new_task_for_function(
    function: &SchedFunction,
    target: &TaskTarget,
    trigger: TaskTrigger,
    parent_tid: Option<i64>,
) -> NewTask {
    NewTask {
        name: function.function.clone(),
        display_name: function.display_name.clone(),
        type_name: function.type_name.clone(),
        evidence_id: target.evidence_id,
        plugin_id: function.plugin_id.clone(),
        analyzer: function.analyzer.clone(),
        trigger,
        parent_tid,
        params: target.params.clone(),
        dependence: function.dependence.clone().unwrap_or_default(),
        can_pause: function.can_pause,
        single_instance: function.single_instance,
        internal: function.internal,
    }
}

/// 证据上线自动建首任务。返回每个 auto 根 function 的建任务结果。
pub fn create_first_tasks(
    store: &TaskStore,
    registry: &FunctionRegistry,
    target: &TaskTarget,
) -> anyhow::Result<Vec<InsertOutcome>> {
    let mut outcomes = Vec::new();
    for function in registry.auto_roots() {
        let new_task = new_task_for_function(function, target, TaskTrigger::Auto, None);
        let outcome = store.insert_task(&new_task)?;
        tracing::info!(
            "eid-DisplayName:{}-{} create auto task: {:?}",
            target.evidence_id,
            function.display_name,
            outcome.record().status
        );
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

/// 任务终结后检查依赖链。只有 `success` 触发下一级建任务。
pub fn check_dependence(
    store: &TaskStore,
    registry: &FunctionRegistry,
    finished: &SchedTaskRecord,
) -> anyhow::Result<Vec<InsertOutcome>> {
    if finished.status != SchedTaskStatus::Success {
        tracing::debug!(
            "CheckDependenceTask task {} ({}) is {}, downstream blocked",
            finished.id,
            finished.name,
            finished.status.as_str()
        );
        return Ok(Vec::new());
    }
    let target = TaskTarget {
        evidence_id: finished.evidence_id,
        params: finished.params.clone(),
    };
    let mut outcomes = Vec::new();
    for dependent in registry.dependents_of(&finished.name) {
        let new_task =
            new_task_for_function(dependent, &target, TaskTrigger::Auto, Some(finished.id));
        let outcome = store.insert_task(&new_task)?;
        match &outcome {
            InsertOutcome::Created(record) => tracing::info!(
                "CheckDependenceTask task {} -> CreateNextTask {} (tid {})",
                finished.name,
                dependent.function,
                record.id
            ),
            InsertOutcome::Existing(record) => tracing::debug!(
                "CheckDependenceTask task {} -> {} already active as tid {}",
                finished.name,
                dependent.function,
                record.id
            ),
        }
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::extensions::task_scheduler::{
        functions::tests::chain_definitions,
        store::tests::{insert_evidence, temp_store},
    };

    fn target(evidence_id: i64) -> TaskTarget {
        TaskTarget {
            evidence_id,
            params: TaskParams {
                node_id: Some("node-9".to_owned()),
                file_name: Some("target.db".to_owned()),
                platform: None,
                sub_items: Vec::new(),
            },
        }
    }

    #[test]
    fn evidence_online_creates_only_auto_roots() {
        let (_dir, store) = temp_store();
        let evidence_id = insert_evidence(&store, "image-1");
        let registry = FunctionRegistry::from_definitions(&chain_definitions());
        let outcomes =
            create_first_tasks(&store, &registry, &target(evidence_id)).expect("first tasks");
        assert_eq!(outcomes.len(), 1);
        let InsertOutcome::Created(first) = &outcomes[0] else {
            panic!("root must be created");
        };
        assert_eq!(first.name, "org.test.first/stage.one");
        assert_eq!(first.info.trigger, TaskTrigger::Auto);
        assert_eq!(first.info.parent_tid, None);
        assert_eq!(first.info.image_id, "image-1");
        assert_eq!(first.type_name, "filesystem");
        assert_eq!(first.params.node_id.as_deref(), Some("node-9"));

        // 再次上线:单实例约束命中已有任务。
        let again =
            create_first_tasks(&store, &registry, &target(evidence_id)).expect("first tasks");
        assert_eq!(again, vec![InsertOutcome::Existing(first.clone())]);
    }

    #[test]
    fn success_creates_dependents_with_parent_link() {
        let (_dir, store) = temp_store();
        let evidence_id = insert_evidence(&store, "image-1");
        let registry = FunctionRegistry::from_definitions(&chain_definitions());
        let mut first = create_first_tasks(&store, &registry, &target(evidence_id))
            .expect("first")
            .remove(0)
            .into_record();
        first = store
            .update_status(first.id, SchedTaskStatus::Success, None)
            .expect("finish")
            .expect("row");

        let next = check_dependence(&store, &registry, &first).expect("dependence");
        assert_eq!(next.len(), 1);
        let InsertOutcome::Created(second) = &next[0] else {
            panic!("dependent must be created");
        };
        assert_eq!(second.name, "org.test.second/stage.two");
        assert_eq!(second.dependence, "org.test.first/stage.one");
        assert_eq!(second.info.parent_tid, Some(first.id));
        assert_eq!(second.info.trigger, TaskTrigger::Auto);
        assert!(second.internal);
        assert_eq!(second.params, first.params);
        assert_eq!(second.evidence_id, evidence_id);
        assert_eq!(second.info.image_id, "image-1");
    }

    #[test]
    fn failure_blocks_downstream_but_success_creates_dependents() {
        let (_dir, store) = temp_store();
        let evidence_id = insert_evidence(&store, "image-1");
        let registry = FunctionRegistry::from_definitions(&chain_definitions());
        let mut first = create_first_tasks(&store, &registry, &target(evidence_id))
            .expect("first")
            .remove(0)
            .into_record();
        first = store
            .update_status(first.id, SchedTaskStatus::Failed, Some("x"))
            .expect("fail")
            .expect("row");
        assert!(check_dependence(&store, &registry, &first)
            .expect("dependence")
            .is_empty());

        first = store
            .update_status(first.id, SchedTaskStatus::Success, None)
            .expect("success")
            .expect("row");
        let next = check_dependence(&store, &registry, &first).expect("dependence");
        assert_eq!(next.len(), 1);
    }
}
