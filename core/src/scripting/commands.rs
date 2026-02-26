#[derive(Debug, Clone)]
pub enum Command {
    DealDamage { target_id: u64, amount: f32 },
    Heal { target_id: u64, amount: f32 },
    ApplyKnockback { target_id: u64, force: f32 },
    SpawnVfx { name: String, target_id: u64 },
    PlaySound { name: String, pos_x: f32, pos_y: f32, pos_z: f32 },
    Animate { entity_id: u64, animation: String },
    AddBuff { target_id: u64, name: String, duration: f32 },
    RemoveBuff { target_id: u64, name: String },
    SetStat { entity_id: u64, stat: String, value: f32 },
    SetBehavior { entity_id: u64, behavior: String },
    MoveToward { entity_id: u64, target_x: f32, target_z: f32, speed: f32 },
    ScreenShake { intensity: f32 },
    HitStop { duration: f32 },
}

#[derive(Debug, Default, Clone)]
pub struct CommandBuffer {
    pub commands: Vec<Command>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cmd: Command) {
        self.commands.push(cmd);
    }

    pub fn drain(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_drain() {
        let mut buf = CommandBuffer::new();
        buf.push(Command::DealDamage {
            target_id: 1,
            amount: 50.0,
        });
        buf.push(Command::Heal {
            target_id: 2,
            amount: 25.0,
        });

        let cmds = buf.drain();
        assert_eq!(cmds.len(), 2);
        assert!(buf.commands.is_empty(), "buffer should be empty after drain");
    }

    #[test]
    fn drain_empty_buffer() {
        let mut buf = CommandBuffer::new();
        let cmds = buf.drain();
        assert!(cmds.is_empty());
    }
}
