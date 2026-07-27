use crate::protocol::error::BudgetDimension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetLimit {
    pub dimension: BudgetDimension,
    pub used: u64,
    pub max: u64,
}
impl BudgetLimit {
    pub fn new(d: BudgetDimension, max: u64) -> Self {
        Self {
            dimension: d,
            used: 0,
            max,
        }
    }
    pub fn consume(&mut self, a: u64) -> Result<(), BudgetDimension> {
        self.used = self.used.saturating_add(a);
        if self.used > self.max {
            Err(self.dimension)
        } else {
            Ok(())
        }
    }
    pub fn is_exhausted(&self) -> bool {
        self.used >= self.max
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBudget {
    pub tokens: BudgetLimit,
    pub tool_calls: BudgetLimit,
    pub write_prepare_limit: usize,
    pub write_commit_limit: usize,
    pub write_prepare_count: usize,
    pub write_commit_count: usize,
}
impl AgentBudget {
    pub fn new(model_limit: u64, _deadline: i64) -> Self {
        let w = ((model_limit as f64) * 0.70) as u64;
        Self {
            tokens: BudgetLimit::new(BudgetDimension::Tokens, w),
            tool_calls: BudgetLimit::new(BudgetDimension::ToolCalls, 40),
            write_prepare_limit: 10,
            write_commit_limit: 5,
            write_prepare_count: 0,
            write_commit_count: 0,
        }
    }
    pub fn consume_tokens(&mut self, a: u64) -> Result<(), BudgetDimension> {
        self.tokens.consume(a)
    }
    pub fn consume_tool_call(&mut self) -> Result<(), BudgetDimension> {
        self.tool_calls.consume(1)
    }
    pub fn record_write_prepare(&mut self) -> Result<(), String> {
        if self.write_prepare_count >= self.write_prepare_limit {
            Err("limit".into())
        } else {
            self.write_prepare_count += 1;
            Ok(())
        }
    }
    pub fn any_exhausted(&self) -> Option<BudgetDimension> {
        if self.tokens.is_exhausted() {
            Some(BudgetDimension::Tokens)
        } else if self.tool_calls.is_exhausted() {
            Some(BudgetDimension::ToolCalls)
        } else {
            None
        }
    }
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn budget_scales() {
        let b = AgentBudget::new(100000, 300);
        assert_eq!(b.tokens.max, 70000);
    }
    #[test]
    fn token_exhaustion() {
        let mut b = AgentBudget::new(1000, 300);
        b.consume_tokens(700).unwrap();
        assert!(b.consume_tokens(1).is_err());
    }
}
