import {
  claimBossChestLive,
  getEconomy,
  getLeagueBracket,
  listBosses,
  rolloverLeague,
  scanRaidTargets,
  setRaidOptIn,
  startAndSettleRaid,
  strikeBossLive,
  type RaidCandidateApi,
} from './api.js';
import type { EvaluateJsonFn } from './controller.js';

export type M6TabKey = 'RAIDS' | 'LEAGUES' | 'BOSSES' | 'CLANS';

export interface RaidScannerSector {
  defender_user_id: string;
  node_id: string;
  node_title: string;
  tier: number;
  e_u_ticks: number;
  raid_immune: boolean;
  immunity_reason: string | null;
  has_revenge_window: boolean;
  revenge_age_seconds: number;
}

export interface LeagueBracketEntry {
  user_id: string;
  trophies_earned: number;
  eligible_mastery_recovery_count: number;
  last_trophy_earned_epoch_ms: number;
  is_dormant: boolean;
  returned_from_dormancy_this_week: boolean;
  qualifying_sessions_since_return: number;
  consecutive_dormant_weeks: number;
  outcome?: string;
}

export interface M6OperationsState {
  activeTab: M6TabKey;
  outboundToday: number;
  inboundToday: number;
  pairRaidsToday: number;
  defenderDailyTrophyLoss: number;
  sectors: RaidScannerSector[];
  selectedSectorIndex: number;
  lastRaidStatus: string;
  leagueTier: number;
  leagueStandings: LeagueBracketEntry[];
  lastRolloverSummary: string;
  bossTitle: string;
  bossMode: 'SOLO' | 'ASYNC_COOP' | 'LIVE_COOP';
  bossMaxHp: number;
  bossCurrentHp: number;
  bossOverkillLast: number;
  bossChestClaimed: boolean;
  clanActiveSnapshot: number;
  clanRawPoints: number;
  clanNormalizedPoints: number;
  goldBalance: number;
  trophyBalance: number;
  practiceXpBalance: number;
  cosmetics: string[];
}

export interface M6OperationsController {
  getState(): M6OperationsState;
  setLiveCourse(courseId: string | null): Promise<void>;
  refreshLiveState(): Promise<void>;
  setTab(tab: M6TabKey): void;
  destroy(): void;
}

function currentIsoWeekKey(): string {
  const now = new Date();
  const date = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()),
  );
  const dayNum = date.getUTCDay() || 7;
  date.setUTCDate(date.getUTCDate() + 4 - dayNum);
  const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
  const weekNo = Math.ceil(
    ((date.getTime() - yearStart.getTime()) / 86400000 + 1) / 7,
  );
  return `${date.getUTCFullYear()}-W${String(weekNo).padStart(2, '0')}`;
}

export function createInitialM6State(): M6OperationsState {
  return {
    activeTab: 'RAIDS',
    outboundToday: 1,
    inboundToday: 1,
    pairRaidsToday: 0,
    defenderDailyTrophyLoss: 15,
    sectors: [
      {
        defender_user_id: 'defender-alpha',
        node_id: 'node-memory-1',
        node_title: 'Cache Coherence Bastion',
        tier: 3,
        e_u_ticks: 8400,
        raid_immune: false,
        immunity_reason: null,
        has_revenge_window: false,
        revenge_age_seconds: 0,
      },
      {
        defender_user_id: 'defender-beta',
        node_id: 'node-pipeline-2',
        node_title: 'Control Hazard Spire',
        tier: 2,
        e_u_ticks: 7100,
        raid_immune: false,
        immunity_reason: null,
        has_revenge_window: true,
        revenge_age_seconds: 1800,
      },
      {
        defender_user_id: 'defender-gamma',
        node_id: 'node-shield-grace',
        node_title: 'Version Grace Vault',
        tier: 1,
        e_u_ticks: 5200,
        raid_immune: true,
        immunity_reason: 'VERSION_GRACE',
        has_revenge_window: false,
        revenge_age_seconds: 0,
      },
    ],
    selectedSectorIndex: 0,
    lastRaidStatus: 'Scanner ready · F-04 allowlist active (no FSRS recall leaks)',
    leagueTier: 2,
    leagueStandings: [
      {
        user_id: 'scholar-1',
        trophies_earned: 185,
        eligible_mastery_recovery_count: 6,
        last_trophy_earned_epoch_ms: 1000,
        is_dormant: false,
        returned_from_dormancy_this_week: false,
        qualifying_sessions_since_return: 0,
        consecutive_dormant_weeks: 0,
      },
      {
        user_id: 'scholar-2',
        trophies_earned: 142,
        eligible_mastery_recovery_count: 9,
        last_trophy_earned_epoch_ms: 900,
        is_dormant: false,
        returned_from_dormancy_this_week: false,
        qualifying_sessions_since_return: 0,
        consecutive_dormant_weeks: 0,
      },
      {
        user_id: 'scholar-dormant',
        trophies_earned: 190,
        eligible_mastery_recovery_count: 2,
        last_trophy_earned_epoch_ms: 800,
        is_dormant: false,
        returned_from_dormancy_this_week: true,
        qualifying_sessions_since_return: 2,
        consecutive_dormant_weeks: 1,
      },
    ],
    lastRolloverSummary: 'Tier 2 · Promotion requires >= 150 trophies',
    bossTitle: 'L4 Chapter Warden',
    bossMode: 'LIVE_COOP',
    bossMaxHp: 250,
    bossCurrentHp: 250,
    bossOverkillLast: 0,
    bossChestClaimed: false,
    clanActiveSnapshot: 9,
    clanRawPoints: 19,
    clanNormalizedPoints: 6,
    goldBalance: 120,
    trophyBalance: 145,
    practiceXpBalance: 15,
    cosmetics: ['cobalt_warden_sigil'],
  };
}

export function mountM6OperationsPanel(
  container: HTMLElement,
  evaluateJson: EvaluateJsonFn,
): M6OperationsController {
  let state = createInitialM6State();
  let liveCourseId: string | null = null;
  let liveBossId: string | null = null;
  let liveBossStrikeSeq = 0;
  let liveCandidates: RaidCandidateApi[] = [];

  container.innerHTML = `
    <div class="m6-ops-panel" data-testid="m6-ops-panel">
      <div class="m4-subhead">
        <h3>M6 Operations Deck</h3>
        <div class="m6-econ-badges" id="m6-econ-badges" data-testid="m6-econ-badges"></div>
      </div>

      <div class="m6-tab-row" role="tablist" aria-label="M6 Operations Tabs">
        <button type="button" class="touch-btn m6-tab-btn" data-m6-tab="RAIDS" data-testid="m6-tab-raids">Raids</button>
        <button type="button" class="touch-btn m6-tab-btn" data-m6-tab="LEAGUES" data-testid="m6-tab-leagues">Leagues</button>
        <button type="button" class="touch-btn m6-tab-btn" data-m6-tab="BOSSES" data-testid="m6-tab-bosses">Bosses</button>
        <button type="button" class="touch-btn m6-tab-btn" data-m6-tab="CLANS" data-testid="m6-tab-clans">Clan &amp; Vault</button>
      </div>

      <div class="m6-tab-body" id="m6-tab-body" data-testid="m6-tab-body"></div>
    </div>
  `;

  const econBadgesEl = container.querySelector<HTMLElement>('#m6-econ-badges')!;
  const tabBodyEl = container.querySelector<HTMLElement>('#m6-tab-body')!;
  const tabButtons = Array.from(container.querySelectorAll<HTMLButtonElement>('.m6-tab-btn'));

  const renderEconBadges = () => {
    econBadgesEl.innerHTML = `
      <span class="badge-chip badge-safe" data-testid="m6-gold-badge">Gold ${state.goldBalance}</span>
      <span class="badge-chip" data-testid="m6-trophy-badge">Trophies ${state.trophyBalance}</span>
      <span class="badge-chip" data-testid="m6-xp-badge">PracticeXP ${state.practiceXpBalance}</span>
    `;
  };

  const refreshLiveState = async () => {
    if (!liveCourseId) return;
    const courseId = liveCourseId;
    const weekKey = currentIsoWeekKey();
    const [econRes, scanRes, bossesRes, leagueRes] = await Promise.allSettled([
      getEconomy(courseId),
      setRaidOptIn(courseId, true).then(() => scanRaidTargets(courseId)),
      listBosses(courseId),
      getLeagueBracket(courseId, weekKey, state.leagueTier),
    ]);

    let next = { ...state };
    if (econRes.status === 'fulfilled') {
      const bal = econRes.value.balance;
      if (bal) {
        next.goldBalance = bal.gold;
        next.trophyBalance = bal.trophies;
        next.practiceXpBalance = bal.practice_xp;
      }
      if (econRes.value.cosmetics.length > 0) {
        next.cosmetics = econRes.value.cosmetics.map(c => c.cosmetic_key);
      }
    }
    if (scanRes.status === 'fulfilled' && scanRes.value.candidates.length > 0) {
      liveCandidates = scanRes.value.candidates;
      next.sectors = scanRes.value.candidates.map(c => ({
        defender_user_id: c.defender_user_id,
        node_id: c.node_id,
        node_title: c.node_title,
        tier: c.tier,
        e_u_ticks: c.e_u_ticks,
        raid_immune: c.raid_immune,
        immunity_reason: c.immunity_reason,
        has_revenge_window: c.has_revenge_window,
        revenge_age_seconds: 0,
      }));
    }
    if (bossesRes.status === 'fulfilled' && bossesRes.value.bosses.length > 0) {
      const activeBoss =
        bossesRes.value.bosses.find(b => b.status === 'ACTIVE') ??
        bossesRes.value.bosses[0];
      if (activeBoss) {
        liveBossId = activeBoss.id;
        next.bossTitle = activeBoss.title;
        next.bossMode = activeBoss.mode;
        next.bossMaxHp = activeBoss.max_hp;
        next.bossCurrentHp = activeBoss.current_hp;
        next.bossChestClaimed =
          next.bossChestClaimed ||
          (Boolean(activeBoss.cosmetic_reward_key) &&
            next.cosmetics.includes(activeBoss.cosmetic_reward_key!));
      }
    }
    if (
      leagueRes.status === 'fulfilled' &&
      leagueRes.value.standings.length > 0
    ) {
      next.leagueStandings = leagueRes.value.standings.map(st => ({
        user_id: st.user_id,
        trophies_earned: st.trophies_earned,
        eligible_mastery_recovery_count: st.eligible_mastery_recovery_count,
        last_trophy_earned_epoch_ms: Date.parse(st.last_trophy_earned_at) || Date.now(),
        is_dormant: st.is_dormant,
        returned_from_dormancy_this_week: st.returned_from_dormancy_this_week,
        qualifying_sessions_since_return: st.qualifying_sessions_since_return,
        consecutive_dormant_weeks: st.consecutive_dormant_weeks,
        ...(st.rollover_outcome ? { outcome: st.rollover_outcome } : {}),
      }));
    }
    state = next;
    render();
  };

  const handleRaidExecution = (sectorIdx: number, isRevenge: boolean) => {
    const sector = state.sectors[sectorIdx];
    if (!sector) return;

    if (liveCourseId) {
      const courseId = liveCourseId;
      const candidate = liveCandidates[sectorIdx];
      void (async () => {
        try {
          const settled = await startAndSettleRaid(courseId, {
            defenderUserId: sector.defender_user_id,
            defenderNodeId: sector.node_id,
            isRevenge,
            revengeSourceRaidId: candidate?.revenge_source_raid_id ?? null,
            sandboxSolved: true,
            drillsPassed: 3,
            baseScorePoints: 28,
          });
          const out = settled.settlement;
          state = {
            ...state,
            selectedSectorIndex: sectorIdx,
            lastRaidStatus: `${out.status} · +${out.attacker_trophy_gain} trophies · +${out.gold_awarded} gold · Chest: ${out.loot_chest_tier ?? 'NONE'}`,
          };
          await refreshLiveState();
        } catch (err) {
          state = {
            ...state,
            selectedSectorIndex: sectorIdx,
            lastRaidStatus: `Blocked (${err instanceof Error ? err.message : 'RAID_FAILED'})`,
          };
          render();
        }
      })();
      return;
    }

    const targRes = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'raid_targetability',
          input: {
            attacker_user_id: 'local-player',
            defender_user_id: sector.defender_user_id,
            course_is_private: true,
            attacker_raid_opt_in: true,
            defender_raid_opt_in: true,
            node_state: 'ACTIVE',
            node_tier: sector.tier,
            active_protections: sector.immunity_reason ? [sector.immunity_reason] : [],
            attacker_outbound_raids_today: state.outboundToday,
            defender_inbound_raids_today: state.inboundToday,
            unordered_pair_raids_today: state.pairRaidsToday,
            is_revenge: isRevenge,
            revenge_source_available: sector.has_revenge_window,
            revenge_age_seconds: sector.revenge_age_seconds,
          },
        }),
      ),
    );

    if (!targRes.result?.targetable) {
      state = {
        ...state,
        selectedSectorIndex: sectorIdx,
        lastRaidStatus: `Blocked (${targRes.result?.blocking_reason ?? 'IMMUNE'})`,
      };
      render();
      return;
    }

    const scoreRes = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'score_v2',
          input: {
            base_points: 20,
            rating: 4,
            recall_bps: 11000,
            mastery_bps: 10500,
            mode_bps: targRes.result.mode_bps,
            recovery_x1e4: 10000,
          },
        }),
      ),
    );

    const settleRes = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'raid_settlement',
          input: {
            is_revenge: isRevenge,
            sandbox_solved: true,
            drills_passed: 3,
            elapsed_from_stage2_ms: 42000,
            defender_immune_at_settle: sector.raid_immune,
            defender_daily_trophy_loss_so_far: state.defenderDailyTrophyLoss,
            attacker_practice_xp_today: state.practiceXpBalance,
            reward_eligible: true,
            base_score_v2_points: Number(scoreRes.result?.points ?? 25),
          },
        }),
      ),
    );

    const out = settleRes.result;
    const nextSectors = state.sectors.map((s, idx) =>
      idx === sectorIdx
        ? {
            ...s,
            raid_immune: true,
            immunity_reason: 'POST_RAID_COOLDOWN',
            has_revenge_window: false,
          }
        : s,
    );

    state = {
      ...state,
      selectedSectorIndex: sectorIdx,
      outboundToday: state.outboundToday + 1,
      inboundToday: out.consumes_defender_inbound_cap ? state.inboundToday + 1 : state.inboundToday,
      pairRaidsToday: 1,
      defenderDailyTrophyLoss: state.defenderDailyTrophyLoss + Number(out.defender_trophy_loss ?? 0),
      goldBalance: state.goldBalance + Number(out.gold_awarded ?? 0),
      trophyBalance: state.trophyBalance + Number(out.attacker_trophy_gain ?? 0),
      practiceXpBalance: state.practiceXpBalance + Number(out.practice_xp_awarded ?? 0),
      sectors: nextSectors,
      lastRaidStatus: `${out.status} · +${out.attacker_trophy_gain} trophies · +${out.gold_awarded} gold · Chest: ${out.loot_chest_tier ?? 'NONE'}`,
    };
    render();
  };

  const handleLeagueRollover = () => {
    if (liveCourseId) {
      const courseId = liveCourseId;
      void (async () => {
        try {
          const summary = await rolloverLeague(
            courseId,
            currentIsoWeekKey(),
            state.leagueTier,
          );
          const outcomeByUser = new Map<string, string>();
          for (const st of summary.standings) {
            if (st.rollover_outcome) {
              outcomeByUser.set(st.user_id, st.rollover_outcome);
            }
          }
          state = {
            ...state,
            leagueStandings: state.leagueStandings.map(entry => ({
              ...entry,
              outcome: outcomeByUser.get(entry.user_id) ?? entry.outcome ?? 'HELD',
            })),
            lastRolloverSummary: `Live Rollover · Ranked=${summary.outcome.ranked_count ?? 0} · PromoSlots=${summary.outcome.max_promo_slots ?? 0}`,
          };
          render();
        } catch (err) {
          state = {
            ...state,
            lastRolloverSummary: `Rollover failed: ${err instanceof Error ? err.message : 'ERROR'}`,
          };
          render();
        }
      })();
      return;
    }

    const res = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'league_rollover',
          input: {
            tier: state.leagueTier,
            late_joiner_shielded: false,
            participants: state.leagueStandings,
          },
        }),
      ),
    );

    const outcomeByUser = new Map<string, string>();
    for (const st of res.result?.standings ?? []) {
      outcomeByUser.set(st.user_id, st.outcome);
    }
    state = {
      ...state,
      leagueStandings: state.leagueStandings.map(entry => ({
        ...entry,
        outcome: outcomeByUser.get(entry.user_id) ?? 'HELD',
      })),
      lastRolloverSummary: `Rollover complete · Ranked=${res.result?.ranked_count ?? 0} · PromoSlots=${res.result?.max_promo_slots ?? 0}`,
    };
    render();
  };

  const handleBossStrike = (overcharged: boolean) => {
    if (state.bossCurrentHp <= 0) return;
    if (liveCourseId && liveBossId) {
      const courseId = liveCourseId;
      const bossId = liveBossId;
      liveBossStrikeSeq += 1;
      const seq = liveBossStrikeSeq;
      void (async () => {
        try {
          const res = await strikeBossLive(courseId, bossId, {
            sequence: seq,
            rating: overcharged ? 4 : 3,
            overcharged,
            baseDamage: 100,
          });
          state = {
            ...state,
            bossCurrentHp: res.strike.boss_hp_after,
            bossOverkillLast: res.strike.overkill_damage,
          };
          render();
        } catch {
          await refreshLiveState();
        }
      })();
      return;
    }

    const res = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'boss_strike',
          input: {
            boss_hp_before: state.bossCurrentHp,
            base_damage: 100,
            rating: overcharged ? 4 : 3,
            overcharged,
          },
        }),
      ),
    );

    const out = res.result;
    state = {
      ...state,
      bossCurrentHp: Number(out.boss_hp_after ?? 0),
      bossOverkillLast: Number(out.overkill_damage ?? 0),
    };
    render();
  };

  const handleBossClaim = () => {
    if (state.bossCurrentHp > 0 || state.bossChestClaimed) return;
    if (liveCourseId && liveBossId) {
      const courseId = liveCourseId;
      const bossId = liveBossId;
      void (async () => {
        try {
          const res = await claimBossChestLive(courseId, bossId);
          state = {
            ...state,
            bossChestClaimed: true,
            cosmetics:
              res.claim.cosmetic_key && !state.cosmetics.includes(res.claim.cosmetic_key)
                ? [...state.cosmetics, res.claim.cosmetic_key]
                : state.cosmetics,
          };
          await refreshLiveState();
        } catch {
          render();
        }
      })();
      return;
    }

    state = {
      ...state,
      bossChestClaimed: true,
      goldBalance: state.goldBalance + 150,
      cosmetics: state.cosmetics.includes('apex_warden_halo')
        ? state.cosmetics
        : [...state.cosmetics, 'apex_warden_halo'],
    };
    render();
  };

  const handleClanStability = () => {
    const res = JSON.parse(
      evaluateJson(
        JSON.stringify({
          kind: 'clan_stability',
          input: {
            s_before: 2.0,
            s_after: 10.0,
            active_members_snapshot: state.clanActiveSnapshot,
            reward_eligible: true,
            member_in_frozen_roster: true,
          },
        }),
      ),
    );
    const out = res.result;
    state = {
      ...state,
      clanRawPoints: state.clanRawPoints + Number(out.raw_points ?? 0),
      clanNormalizedPoints: state.clanNormalizedPoints + Number(out.normalized_points ?? 0),
    };
    render();
  };

  const renderRaidsTab = () => {
    const caps = document.createElement('div');
    caps.className = 'm6-caps-row';
    caps.textContent = `Daily Caps — Outbound: ${state.outboundToday}/10 · Defender Loss: ${state.defenderDailyTrophyLoss}/45`;

    const status = document.createElement('div');
    status.className = 'm4-muted-note';
    status.dataset.testid = 'm6-raid-status';
    status.textContent = state.lastRaidStatus;

    const list = document.createElement('div');
    list.className = 'm6-card-list';

    state.sectors.forEach((sec, idx) => {
      const card = document.createElement('div');
      card.className = 'm6-item-row';

      const info = document.createElement('div');
      info.className = 'm6-item-info';
      info.innerHTML = `<strong>${sec.node_title}</strong> <span>T${sec.tier} · E_u=${sec.e_u_ticks}${sec.raid_immune ? ` · IMMUNE (${sec.immunity_reason})` : ' · VULNERABLE'}</span>`;

      const actions = document.createElement('div');
      actions.className = 'm6-item-actions';

      const raidBtn = document.createElement('button');
      raidBtn.type = 'button';
      raidBtn.className = 'touch-btn accent-btn';
      raidBtn.dataset.testid = `m6-raid-exec-${idx}`;
      raidBtn.textContent = sec.has_revenge_window ? 'Revenge 1.5x' : 'Scout & Raid';
      raidBtn.addEventListener('click', () => handleRaidExecution(idx, sec.has_revenge_window));

      actions.append(raidBtn);
      card.append(info, actions);
      list.append(card);
    });

    tabBodyEl.replaceChildren(caps, status, list);
  };

  const renderLeaguesTab = () => {
    const summary = document.createElement('div');
    summary.className = 'm4-muted-note';
    summary.dataset.testid = 'm6-league-summary';
    summary.textContent = state.lastRolloverSummary;

    const list = document.createElement('div');
    list.className = 'm6-card-list';
    state.leagueStandings.forEach(entry => {
      const row = document.createElement('div');
      row.className = 'm6-item-row';
      row.innerHTML = `<div class="m6-item-info"><strong>${entry.user_id}</strong> <span>${entry.trophies_earned} trophies · Rec=${entry.eligible_mastery_recovery_count}</span></div><span class="badge-chip">${entry.outcome ?? 'ACTIVE'}</span>`;
      list.append(row);
    });

    const rolloverBtn = document.createElement('button');
    rolloverBtn.type = 'button';
    rolloverBtn.className = 'touch-btn accent-btn';
    rolloverBtn.dataset.testid = 'm6-league-rollover-btn';
    rolloverBtn.textContent = 'Evaluate Weekly Rollover';
    rolloverBtn.addEventListener('click', handleLeagueRollover);

    tabBodyEl.replaceChildren(summary, list, rolloverBtn);
  };

  const renderBossesTab = () => {
    const bar = document.createElement('div');
    bar.className = 'm6-item-row';
    bar.innerHTML = `<div class="m6-item-info"><strong>${state.bossTitle} (${state.bossMode})</strong> <span data-testid="m6-boss-hp">HP: ${state.bossCurrentHp} / ${state.bossMaxHp} · Last Overkill Clamped: ${state.bossOverkillLast}</span></div>`;

    const actions = document.createElement('div');
    actions.className = 'm6-item-actions';

    const strikeBtn = document.createElement('button');
    strikeBtn.type = 'button';
    strikeBtn.className = 'touch-btn accent-btn';
    strikeBtn.dataset.testid = 'm6-boss-strike-btn';
    strikeBtn.textContent = 'Overcharge Boss Strike (200 DMG)';
    strikeBtn.addEventListener('click', () => handleBossStrike(true));

    const claimBtn = document.createElement('button');
    claimBtn.type = 'button';
    claimBtn.className = 'touch-btn';
    claimBtn.dataset.testid = 'm6-boss-claim-btn';
    claimBtn.textContent = state.bossChestClaimed ? 'Chest Claimed' : 'Claim Boss Chest';
    claimBtn.disabled = state.bossCurrentHp > 0 || state.bossChestClaimed;
    claimBtn.addEventListener('click', handleBossClaim);

    actions.append(strikeBtn, claimBtn);
    tabBodyEl.replaceChildren(bar, actions);
  };

  const renderClansTab = () => {
    const summary = document.createElement('div');
    summary.className = 'm6-item-row';
    summary.innerHTML = `<div class="m6-item-info"><strong>Frozen Week-Start Roster: ${state.clanActiveSnapshot} members</strong> <span data-testid="m6-clan-pts">Raw: ${state.clanRawPoints} · Normalized: ${state.clanNormalizedPoints}</span></div>`;

    const vault = document.createElement('div');
    vault.className = 'm4-muted-note';
    vault.dataset.testid = 'm6-cosmetics-list';
    vault.textContent = `Cosmetic Vault (${state.cosmetics.length}): ${state.cosmetics.join(', ')}`;

    const contribBtn = document.createElement('button');
    contribBtn.type = 'button';
    contribBtn.className = 'touch-btn accent-btn';
    contribBtn.dataset.testid = 'm6-clan-contrib-btn';
    contribBtn.textContent = 'Commit Relative Stability (2.0 -> 10.0)';
    contribBtn.addEventListener('click', handleClanStability);

    tabBodyEl.replaceChildren(summary, vault, contribBtn);
  };

  const render = () => {
    renderEconBadges();
    for (const btn of tabButtons) {
      const key = btn.dataset.m6Tab as M6TabKey;
      btn.setAttribute('aria-pressed', key === state.activeTab ? 'true' : 'false');
    }
    if (state.activeTab === 'RAIDS') {
      renderRaidsTab();
    } else if (state.activeTab === 'LEAGUES') {
      renderLeaguesTab();
    } else if (state.activeTab === 'BOSSES') {
      renderBossesTab();
    } else {
      renderClansTab();
    }
  };

  for (const btn of tabButtons) {
    btn.addEventListener('click', () => {
      const next = btn.dataset.m6Tab as M6TabKey | undefined;
      if (next) {
        state = { ...state, activeTab: next };
        render();
      }
    });
  }

  render();

  return {
    getState: () => state,
    setLiveCourse: async (courseId: string | null) => {
      liveCourseId = courseId;
      liveBossId = null;
      liveCandidates = [];
      if (courseId) {
        await refreshLiveState();
      } else {
        render();
      }
    },
    refreshLiveState,
    setTab: (tab: M6TabKey) => {
      state = { ...state, activeTab: tab };
      render();
    },
    destroy: () => {
      container.replaceChildren();
    },
  };
}
