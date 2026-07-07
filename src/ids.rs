//! Strongly-typed entity/definition IDs (design §2).
//!
//! Every ID is a thin `u32` newtype. The C++ used raw `UINT` positional
//! indices with a per-family `Invalid*ID = (T)-1` sentinel
//! (libs-map §2.2, §4.1); the port instead represents "no id"/"none"
//! as `Option<IdType>` everywhere a C++ call site compared against the
//! sentinel — there is no in-band invalid value.

macro_rules! def_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Debug)]
        pub struct $name(pub u32);

        impl $name {
            #[inline]
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            #[inline]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(value: u32) -> Self {
                Self(value)
            }
        }
    };
}

def_id!(
    /// Identifies one live robot instance in `sim::World` (`std::map<tRobotID, cAIRobot*>`
    /// key in the original, source-map §2).
    RobotId
);
def_id!(
    /// Identifies one live center (ammo/energy/health) instance in `sim::World`.
    CenterId
);
def_id!(
    /// 0-based index into `GameData.robot_types` (libs-map §2.2 `tRobotTypeID`).
    RobotTypeId
);
def_id!(
    /// 0-based index into `GameData.ai_robot_types` (libs-map §2.2 `tAIRobotTypeID`).
    AiRobotTypeId
);
def_id!(
    /// 0-based index into `GameData.projectile_types` (libs-map §2.2 `tProjectileTypeID`).
    ProjectileTypeId
);
def_id!(
    /// 0-based index into `GameData.particle_types` (libs-map §2.2 `tParticleTypeID`).
    ParticleTypeId
);
def_id!(
    /// 0-based index into `GameData.particle_emitter_types`
    /// (libs-map §2.2 `tParticleEmitterTypeID`).
    EmitterTypeId
);
def_id!(
    /// 0-based index into `GameData.particle_effect_definitions`
    /// (libs-map §2.2 `tParticleEffectDefinitionID`).
    EffectDefId
);
def_id!(
    /// 0-based index into `GameData.game_definitions` (libs-map §2.2 `tGameDefinitionID`).
    GameDefId
);
def_id!(
    /// 0-based index into a `GameDefinition`'s team list (libs-map §2.2 `tTeamDefinitionID`).
    TeamId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_ordered_by_value() {
        assert!(RobotId(1) < RobotId(2));
        assert_eq!(RobotId::new(5).get(), 5);
    }

    #[test]
    fn option_represents_the_cpp_invalid_sentinel() {
        let none: Option<RobotTypeId> = None;
        let some: Option<RobotTypeId> = Some(RobotTypeId(3));
        assert!(none.is_none());
        assert_eq!(some, Some(RobotTypeId(3)));
    }
}
