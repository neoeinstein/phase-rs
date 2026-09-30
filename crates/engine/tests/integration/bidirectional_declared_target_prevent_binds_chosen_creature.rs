//! "Prevent all combat damage that would be dealt to and dealt by target
//! creature you control this turn" (Cephalid Illusionist, Soratami Cloud
//! Chariot), driven through the real activation + combat pipeline.
//!
//! The "by" half must shield only the creature chosen as the target, not every
//! creature the declared filter ("creature you control") matches.
//!
//! CR 601.2c + CR 608.2c + CR 615.1a.

use engine::game::combat::AttackTarget;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::identifiers::ObjectId;
use engine::types::phase::Phase;

const SHIELD: &str = "{T}: Prevent all combat damage that would be dealt to and dealt by target creature you control this turn.";

fn damage_marked(runner: &GameRunner, obj: ObjectId) -> u32 {
    runner.state().objects[&obj].damage_marked
}

/// The chosen attacker deals no combat damage to its blocker and takes none from
/// it; a second attacker of the same controller, matching the same declared
/// filter, still deals its damage.
#[test]
fn declared_target_shield_covers_only_the_chosen_creature_in_both_directions() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let source = scenario
        .add_land_from_oracle(P0, "Shield Source", SHIELD)
        .id();
    let chosen = scenario.add_creature(P0, "Chosen Attacker", 2, 3).id();
    let chosen_blocker = scenario.add_creature(P1, "Chosen Blocker", 2, 3).id();
    let other = scenario.add_creature(P0, "Other Attacker", 2, 3).id();
    let other_blocker = scenario.add_creature(P1, "Other Blocker", 2, 3).id();

    let mut runner = scenario.build();
    runner.advance_to_combat();
    runner
        .declare_attackers(&[
            (chosen, AttackTarget::Player(P1)),
            (other, AttackTarget::Player(P1)),
        ])
        .expect("declaring both attackers must be accepted");
    runner.activate(source, 0).target_object(chosen).resolve();
    runner.pass_both_players();
    runner
        .declare_blockers(&[(chosen_blocker, chosen), (other_blocker, other)])
        .expect("declaring both blocks must be accepted");
    runner.combat_damage();

    assert_eq!(damage_marked(&runner, chosen), 0, "'to' half");
    assert_eq!(damage_marked(&runner, chosen_blocker), 0, "'by' half");
    // Reach guard: the same declared filter matches `other`, whose damage to its
    // blocker must still be dealt (the "by" half is not shared across the filter).
    assert_eq!(
        damage_marked(&runner, other_blocker),
        2,
        "'by' half is scoped"
    );
}
