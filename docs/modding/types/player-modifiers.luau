-- Supplemental types. Handles and expiry ticks are opaque host-issued identities.
export type PlayerModifierOptions = {
    speed: number?, sprint: number?, jump: number?, gravity: number?,
    duration_ticks: number?,
}
export type PlayerModifier = {
    key: string, speed: number, sprint: number, jump: number, gravity: number,
    expires_at: userdata?,
}
export type PlayerModifiersHost = {
    set_player_modifier: (target: userdata, key: string, options: PlayerModifierOptions) -> (),
    remove_player_modifier: (target: userdata, key: string) -> (),
    player_modifiers: (target: userdata) -> {PlayerModifier},
}
return {}
