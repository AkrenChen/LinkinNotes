use std::collections::BTreeSet;

#[derive(Default)]
pub struct ClientStartState { previous: BTreeSet<u32> }

impl ClientStartState {
    pub fn observe(&mut self, current: BTreeSet<u32>, launch: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if current.iter().any(|id| !self.previous.contains(id)) { launch()?; }
        self.previous = current;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts_once_per_client_run_and_detects_restart_without_a_gap() {
        let mut state = ClientStartState::default();
        let mut launches = 0;
        for ids in [vec![], vec![10], vec![10], vec![20], vec![], vec![30], vec![30]] {
            state.observe(ids.into_iter().collect(), || { launches += 1; Ok(()) }).unwrap();
        }
        assert_eq!(launches, 3);
    }
    #[test]
    fn failed_launch_retries_without_resetting_an_existing_client() {
        let mut state = ClientStartState::default();
        let ids = BTreeSet::from([10]);
        assert!(state.observe(ids.clone(), || Err("failed".into())).is_err());
        let mut launches = 0;
        state.observe(ids.clone(), || { launches += 1; Ok(()) }).unwrap();
        state.observe(ids, || panic!("must not relaunch after user exits popup")).unwrap();
        assert_eq!(launches, 1);
    }
}
