use super::*;

impl SkeletonState {
    /// Cancels an in-flight attack committed to an intercepting defense.
    pub fn commit_attack_to_defense(&mut self) -> bool {
        let committed = self.action_kind() == SkeletonAction::Attack;
        if committed {
            self.action = ActionState::default();
        }
        committed
    }
}
