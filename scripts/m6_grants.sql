-- Least-privilege role grants for Milestone M6 Raids, Economy, Leagues, Bosses, and Clan Wars.

REVOKE ALL ON
    reward_transactions,
    user_economy_balances,
    user_cosmetic_inventory,
    raid_sessions,
    notification_delivery_ledger,
    league_brackets,
    league_standings,
    chapter_bosses,
    boss_lobbies,
    boss_contributions,
    boss_chest_claims,
    clans,
    clan_memberships,
    clan_wars,
    clan_war_contributions
FROM PUBLIC, campuslingo_auth;

GRANT SELECT ON
    reward_transactions,
    user_economy_balances,
    user_cosmetic_inventory,
    raid_sessions,
    notification_delivery_ledger,
    boss_contributions,
    boss_chest_claims,
    clan_war_contributions
TO campuslingo_app;

GRANT SELECT, INSERT, UPDATE ON
    league_brackets,
    league_standings,
    chapter_bosses,
    boss_lobbies,
    clans,
    clan_memberships,
    clan_wars
TO campuslingo_app;

GRANT SELECT, INSERT, UPDATE ON
    reward_transactions,
    user_economy_balances,
    user_cosmetic_inventory,
    raid_sessions,
    notification_delivery_ledger,
    league_brackets,
    league_standings,
    chapter_bosses,
    boss_lobbies,
    boss_contributions,
    boss_chest_claims,
    clans,
    clan_memberships,
    clan_wars,
    clan_war_contributions
TO campuslingo_worker, campuslingo_recovery, campuslingo_grader;

GRANT EXECUTE ON FUNCTION user_local_week_start(TIMESTAMPTZ, TEXT) TO campuslingo_app, campuslingo_grader, campuslingo_worker, campuslingo_recovery;
GRANT EXECUTE ON FUNCTION set_course_raid_opt_in_secure(UUID, BOOLEAN) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION list_raid_candidates_secure(UUID, TIMESTAMPTZ) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION start_raid_session_secure(UUID, UUID, UUID, UUID, BOOLEAN, UUID, TEXT, TIMESTAMPTZ) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION settle_raid_session_secure(UUID, UUID, BOOLEAN, SMALLINT, INTEGER, TIMESTAMPTZ) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION apply_boss_strike_secure(UUID, UUID, UUID, INTEGER, SMALLINT, BOOLEAN, INTEGER, TEXT) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION claim_boss_chest_secure(UUID, UUID, TEXT) TO campuslingo_app;
GRANT EXECUTE ON FUNCTION record_clan_war_contribution_secure(UUID, UUID, TEXT, INTEGER, INTEGER, INTEGER, TEXT) TO campuslingo_app;
