# Development Notes

## Current Tasks

### 1. Fantasy Props Mega Kit Integration
- Download from: https://quaternius.com/packs/fantasypropsmegakit.html
- Models to add:
  - [ ] Dummy (training dummy for combat testing)
  - [ ] Sword (player weapon)

### 2. Sword Combat Animations
- Use Universal Animation Library 2 sword animations (now compatible after bone remapping)
- Animations to integrate:
  - [ ] Sword_Regular_A
  - [ ] Sword_Regular_B
  - [ ] Sword_Regular_C
- Will need to update attack system to cycle through sword animations

### 3. PROTOTYPE-style Movement Research
Goal: Movement that feels like PROTOTYPE 1/2 games

Key characteristics to research:
- [ ] Momentum-based movement (acceleration/deceleration curves)
- [ ] High mobility - parkour, wall running, air dashes
- [ ] "Weightiness" combined with responsiveness
- [ ] Camera follows movement with slight lag
- [ ] Ground pound / dive mechanics
- [ ] Sprint that builds up speed over time
- [ ] Fluid animation transitions
- [ ] Combat movement cancels (attack into dash, dash into attack)

Implementation ideas:
- Adjust Tnua parameters for more momentum
- Add velocity-based animation blending
- Camera smoothing/lag for dynamic feel
- Variable sprint speed (ramps up over time)

References:
- PROTOTYPE 1 (2009) - Radical Entertainment
- PROTOTYPE 2 (2012) - Radical Entertainment
- Similar feel: Infamous, Spider-Man PS4, Warframe
