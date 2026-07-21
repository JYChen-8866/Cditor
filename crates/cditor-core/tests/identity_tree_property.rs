//! P1-012：随机 tree/order 操作下的结构不变量 property test。
//!
//! 用确定性 LCG 驱动的模型状态机执行随机 insert/remove/move/reorder（基于
//! `OrderKey`）序列，每步后独立校验：无环、ID 唯一、parent 引用存在、
//! sibling `OrderKey` 严格全序且与模型顺序一致、深度与 parent 链一致。
//! 校验器不复用被测代码的遍历逻辑。

use std::collections::{HashMap, HashSet};

use cditor_core::identity::{OrderKey, rebalanced_keys};

/// 模型节点：ID、parent 与顺序键。
#[derive(Debug, Clone)]
struct ModelNode {
    id: u64,
    parent: Option<u64>,
    key: OrderKey,
}

/// 参考模型：平坦节点表 + 独立校验器。
#[derive(Debug, Default)]
struct TreeModel {
    nodes: Vec<ModelNode>,
    next_id: u64,
}

impl TreeModel {
    fn ids(&self) -> Vec<u64> {
        self.nodes.iter().map(|node| node.id).collect()
    }

    fn children_sorted(&self, parent: Option<u64>) -> Vec<&ModelNode> {
        let mut children: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| node.parent == parent)
            .collect();
        children.sort_by(|a, b| a.key.cmp(&b.key));
        children
    }

    /// 在 parent 的第 position 个位置插入新节点。
    fn insert(&mut self, parent: Option<u64>, position: usize) -> u64 {
        let siblings = self.children_sorted(parent);
        let position = position.min(siblings.len());
        let lower = position
            .checked_sub(1)
            .map(|index| siblings[index].key.clone());
        let upper = siblings.get(position).map(|node| node.key.clone());
        let key = OrderKey::between(lower.as_ref(), upper.as_ref())
            .expect("model bounds are always ordered");
        self.next_id += 1;
        let id = self.next_id;
        self.nodes.push(ModelNode { id, parent, key });
        id
    }

    /// 删除节点及其整个子树。
    fn remove_subtree(&mut self, id: u64) {
        let mut doomed = HashSet::from([id]);
        // 平坦表上迭代传播：直到没有新的后代加入。
        loop {
            let before = doomed.len();
            for node in &self.nodes {
                if let Some(parent) = node.parent
                    && doomed.contains(&parent)
                {
                    doomed.insert(node.id);
                }
            }
            if doomed.len() == before {
                break;
            }
        }
        self.nodes.retain(|node| !doomed.contains(&node.id));
    }

    /// 把节点移到新 parent 的第 position 位；禁止移入自身子树（返回 false）。
    fn move_node(&mut self, id: u64, new_parent: Option<u64>, position: usize) -> bool {
        if let Some(parent) = new_parent {
            if parent == id || self.is_descendant(parent, id) {
                return false;
            }
            if !self.nodes.iter().any(|node| node.id == parent) {
                return false;
            }
        }
        let siblings: Vec<(u64, OrderKey)> = self
            .children_sorted(new_parent)
            .into_iter()
            .filter(|node| node.id != id)
            .map(|node| (node.id, node.key.clone()))
            .collect();
        let position = position.min(siblings.len());
        let lower = position
            .checked_sub(1)
            .map(|index| siblings[index].1.clone());
        let upper = siblings.get(position).map(|(_, key)| key.clone());
        let key = OrderKey::between(lower.as_ref(), upper.as_ref())
            .expect("model bounds are always ordered");
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .expect("caller picks existing id");
        node.parent = new_parent;
        node.key = key;
        true
    }

    /// 对某个 parent 的全部 children 执行局部 rebalance。
    fn rebalance_children(&mut self, parent: Option<u64>) {
        let ordered: Vec<u64> = self
            .children_sorted(parent)
            .into_iter()
            .map(|node| node.id)
            .collect();
        let fresh = rebalanced_keys(ordered.len());
        for (id, key) in ordered.into_iter().zip(fresh) {
            self.nodes
                .iter_mut()
                .find(|node| node.id == id)
                .expect("rebalance target exists")
                .key = key;
        }
    }

    fn is_descendant(&self, candidate: u64, ancestor: u64) -> bool {
        let by_id: HashMap<u64, Option<u64>> = self
            .nodes
            .iter()
            .map(|node| (node.id, node.parent))
            .collect();
        let mut cursor = Some(candidate);
        let mut hops = 0;
        while let Some(current) = cursor {
            if current == ancestor {
                return true;
            }
            cursor = by_id.get(&current).copied().flatten();
            hops += 1;
            assert!(
                hops <= self.nodes.len() + 1,
                "cycle while walking ancestors"
            );
        }
        false
    }
}

/// 独立校验器：不复用模型的排序/遍历实现。
fn assert_invariants(model: &TreeModel, step: usize) {
    // ID 唯一。
    let ids = model.ids();
    let unique: HashSet<_> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "step {step}: duplicate ids");

    let by_id: HashMap<u64, &ModelNode> = model.nodes.iter().map(|node| (node.id, node)).collect();

    for node in &model.nodes {
        // parent 引用存在。
        if let Some(parent) = node.parent {
            assert!(
                by_id.contains_key(&parent),
                "step {step}: dangling parent {parent}"
            );
        }
        // 无环：沿 parent 链最多走 |nodes| 步必须到根。
        let mut cursor = node.parent;
        let mut hops = 0;
        while let Some(current) = cursor {
            assert_ne!(current, node.id, "step {step}: cycle through {}", node.id);
            hops += 1;
            assert!(
                hops <= model.nodes.len(),
                "step {step}: parent chain longer than node count"
            );
            cursor = by_id[&current].parent;
        }
        // OrderKey 结构不变量。
        assert!(!node.key.as_bytes().is_empty());
        assert_ne!(node.key.as_bytes().last(), Some(&0));
    }

    // 每个 parent 的 sibling key 严格全序（无重复）。
    let mut parents: HashSet<Option<u64>> = model.nodes.iter().map(|node| node.parent).collect();
    parents.insert(None);
    for parent in parents {
        let mut keys: Vec<&OrderKey> = model
            .nodes
            .iter()
            .filter(|node| node.parent == parent)
            .map(|node| &node.key)
            .collect();
        keys.sort();
        for pair in keys.windows(2) {
            assert!(
                pair[0] < pair[1],
                "step {step}: sibling keys not strictly ordered under {parent:?}"
            );
        }
    }
}

/// 确定性 LCG。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

fn run_randomized_session(seed: u64, steps: usize) {
    let mut rng = Lcg(seed);
    let mut model = TreeModel::default();
    // 起始若干根节点。
    for _ in 0..4 {
        model.insert(None, usize::MAX);
    }

    for step in 0..steps {
        let action = rng.next(100);
        let ids = model.ids();
        match action {
            // 45%：插入（根或随机 parent）。
            0..=44 => {
                let parent = if ids.is_empty() || rng.next(3) == 0 {
                    None
                } else {
                    Some(ids[rng.next(ids.len())])
                };
                let position = rng.next(8);
                model.insert(parent, position);
            }
            // 20%：移动。
            45..=64 => {
                if ids.len() >= 2 {
                    let id = ids[rng.next(ids.len())];
                    let parent = if rng.next(4) == 0 {
                        None
                    } else {
                        Some(ids[rng.next(ids.len())])
                    };
                    let position = rng.next(8);
                    let _ = model.move_node(id, parent, position);
                }
            }
            // 15%：删除子树（保持模型规模）。
            65..=79 => {
                if ids.len() > 8 {
                    model.remove_subtree(ids[rng.next(ids.len())]);
                }
            }
            // 10%：同 parent 内 reorder（移动到随机新位置）。
            80..=89 => {
                if !ids.is_empty() {
                    let id = ids[rng.next(ids.len())];
                    let parent = model
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .and_then(|node| node.parent);
                    let position = rng.next(12);
                    let _ = model.move_node(id, parent, position);
                }
            }
            // 10%：局部 rebalance。
            _ => {
                let parent = if ids.is_empty() || rng.next(2) == 0 {
                    None
                } else {
                    Some(ids[rng.next(ids.len())])
                };
                model.rebalance_children(parent);
            }
        }
        assert_invariants(&model, step);
    }

    assert!(!model.nodes.is_empty(), "session should end with content");
}

#[test]
fn randomized_tree_and_order_operations_preserve_invariants() {
    for seed in [0x5eed, 0xbead, 0xfeed_beef, 42, 7_777_777] {
        run_randomized_session(seed, 600);
    }
}

#[test]
fn deep_nesting_and_heavy_reorder_at_one_gap() {
    let mut model = TreeModel::default();
    // 深链：每层一个节点。
    let mut parent = None;
    for _ in 0..64 {
        parent = Some(model.insert(parent, 0));
    }
    assert_invariants(&model, 0);

    // 同一间隙反复头插，制造最深 key，然后 rebalance 恢复短 key。
    let deep_parent = parent;
    for _ in 0..128 {
        model.insert(deep_parent, 0);
    }
    assert_invariants(&model, 1);
    let max_depth_before = model
        .children_sorted(deep_parent)
        .iter()
        .map(|node| node.key.as_bytes().len())
        .max()
        .unwrap();
    model.rebalance_children(deep_parent);
    assert_invariants(&model, 2);
    let max_depth_after = model
        .children_sorted(deep_parent)
        .iter()
        .map(|node| node.key.as_bytes().len())
        .max()
        .unwrap();
    assert!(
        max_depth_after <= max_depth_before,
        "rebalance should not deepen keys ({max_depth_before} -> {max_depth_after})"
    );
    assert!(max_depth_after <= 2, "128 siblings fit in two bytes");
}

#[test]
fn move_into_own_subtree_is_rejected_and_state_unchanged() {
    let mut model = TreeModel::default();
    let root = model.insert(None, 0);
    let child = model.insert(Some(root), 0);
    let grandchild = model.insert(Some(child), 0);

    let snapshot: Vec<(u64, Option<u64>)> = model
        .nodes
        .iter()
        .map(|node| (node.id, node.parent))
        .collect();

    assert!(!model.move_node(root, Some(grandchild), 0));
    assert!(!model.move_node(root, Some(root), 0));

    let after: Vec<(u64, Option<u64>)> = model
        .nodes
        .iter()
        .map(|node| (node.id, node.parent))
        .collect();
    assert_eq!(snapshot, after, "rejected moves must not mutate");
    assert_invariants(&model, 0);
}
