-- Supplemental package authoring types; opaque identities are host-issued values.
export type ProfileId = userdata
export type SessionId = userdata
export type Player = {
    profile: ProfileId,
    session: SessionId,
    name: string,
    position: {number},
    online: boolean,
    identity_trust: "claimed_profile",
}
export type RegionEvent = {
    kind: "RegionEntered" | "RegionLeft",
    region: string,
    player: Player,
    profile: ProfileId,
    transition: userdata,
    state: string,
    public_state: string,
    session_state: string,
}
export type ChatRequest = {
    sender: Player,
    text: string,
    players: {Player},
    identity_trust: "claimed_profile",
}
export type ChatDecision = {
    allow: true,
    text: string,
    recipients: {Player}?,
} | {
    allow: false,
    reason: string,
}
export type PlayerWorldStartup = {
    register_region: (key: string, minimum: {number}, maximum: {number}, service: string) -> (),
    register_chat_hook: (key: string, revision: number, module: string) -> (),
}
return {}
