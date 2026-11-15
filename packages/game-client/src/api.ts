import type { DuelOpponentKind, LiveDuelArenaProjection } from './duel.js';

export class ApiError extends Error {
  readonly status: number;
  readonly body: unknown;

  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

export interface ApiUser {
  user_id: string;
}

export interface CourseSummary {
  id: string;
  tenant_id: string;
  title?: string;
  name?: string;
  slug?: string;
  visibility?: string;
  published?: boolean;
}

export interface CachedNodeShieldApi {
  state: string;
  tier: number | null;
  e_u_ticks: number | null;
  i_u_ticks: number | null;
  s_u_ticks: number | null;
  is_brownout: boolean | null;
  raid_immune: boolean;
  propagation_exempt: boolean;
  active_protections: string[];
}

export interface FortressNodeApi {
  node_id: string;
  title: string;
  world_theme: string | null;
  archetype: string | null;
  modifier: string | null;
  grid_x: number | null;
  grid_y: number | null;
  shield: CachedNodeShieldApi | null;
}

export interface FortressEdgeApi {
  prerequisite_node_id: string;
  dependent_node_id: string;
  weight: number;
}

export interface CourseShieldDashboardApi {
  course_id: string;
  tenant_id: string;
  active_content_version_id: string | null;
  updating: boolean;
  computed_at: string | null;
  nodes: FortressNodeApi[];
  edges: FortressEdgeApi[];
}

export interface NodeReportOutcomeApi {
  report_id: string;
  content_version_id: string;
  is_eligible: boolean;
  refund_applied: boolean;
  quarantined?: boolean;
}

export interface MatchmakeDuelResponse {
  match_id: string;
  side: 'PLAYER_A' | 'PLAYER_B';
  opponent_kind: DuelOpponentKind;
  ghost_source_match_id?: string | null;
  arena: LiveDuelArenaProjection;
}

export type DuelClientCommand =
  | { type: 'HEARTBEAT'; client_ts_ms: number }
  | { type: 'START_ROUND'; sequence: number }
  | { type: 'SELECT_CARD'; sequence: number; card_id: string }
  | {
      type: 'STRIKE';
      sequence: number;
      lane: number;
      card_id: string;
      selected_index: number;
      use_quick_blitz_chain?: boolean;
      client_elapsed_ms: number;
      transport_loss_fizzle?: boolean;
      rtt_ms?: number;
    }
  | { type: 'TICK_ADVANCE'; delta_ms: number };

export interface DuelServerFrame {
  type: 'MATCH_STATE' | 'SNAPSHOT_RESYNC' | 'STRIKE_RESOLVED' | 'HEARTBEAT_ACK' | 'ERROR';
  seq?: number;
  match_id?: string;
  payload?: unknown;
}

export interface RaidCandidateApi {
  defender_user_id: string;
  node_id: string;
  node_title: string;
  tier: number;
  e_u_ticks: number;
  raid_immune: boolean;
  immunity_reason: string | null;
  has_revenge_window: boolean;
  revenge_source_raid_id: string | null;
  revenge_window_expires_at: string | null;
}

export interface RaidScannerApi {
  course_id: string;
  limits: {
    outbound_per_day?: number;
    inbound_per_day?: number;
    unordered_pair_per_day?: number;
  };
  candidates: RaidCandidateApi[];
}

export interface SettledRaidApi {
  status: string;
  concurrent_cooldown_blocked: boolean;
  attacker_trophy_gain: number;
  defender_trophy_loss: number;
  practice_xp_awarded: number;
  gold_awarded: number;
  loot_chest_tier: string | null;
  consumes_defender_inbound_cap: boolean;
}

export interface LeagueBracketApi {
  bracket: {
    id: string;
    week_key: string;
    tier: number;
    rolled_over: boolean;
  } | null;
  standings: Array<{
    user_id: string;
    trophies_earned: number;
    eligible_mastery_recovery_count: number;
    last_trophy_earned_at: string;
    is_dormant: boolean;
    returned_from_dormancy_this_week: boolean;
    qualifying_sessions_since_return: number;
    consecutive_dormant_weeks: number;
    rollover_outcome: string | null;
  }>;
}

export interface ChapterBossApi {
  id: string;
  course_id: string;
  node_id: string;
  title: string;
  mode: 'SOLO' | 'ASYNC_COOP' | 'LIVE_COOP';
  max_hp: number;
  current_hp: number;
  reward_tier: string;
  gold_reward: number;
  cosmetic_reward_key: string | null;
  status: string;
}

export interface BossStrikeResponseApi {
  strike: {
    raw_damage: number;
    applied_damage: number;
    overkill_damage: number;
    boss_hp_after: number;
    boss_status: string;
  };
}

export interface BossChestClaimResponseApi {
  claim: {
    claimed_now: boolean;
    reward_tier: string;
    gold_awarded: number;
    cosmetic_key: string | null;
  };
}

export interface EconomyOverviewApi {
  balance: {
    user_id: string;
    course_id: string;
    gold: number;
    trophies: number;
    gems: number;
    practice_xp: number;
    daily_practice_xp: number;
  } | null;
  cosmetics: Array<{
    cosmetic_key: string;
    rarity: string;
  }>;
}

async function requestJson<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body !== undefined && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const response = await fetch(path, {
    ...init,
    credentials: 'same-origin',
    headers,
  });

  if (response.status === 204) {
    return undefined as T;
  }

  const text = await response.text();
  let parsed: unknown = null;
  if (text.length > 0) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = text;
    }
  }

  if (!response.ok) {
    const message =
      typeof parsed === 'object' &&
      parsed !== null &&
      'error' in parsed &&
      typeof (parsed as { error?: unknown }).error === 'string'
        ? (parsed as { error: string }).error
        : `HTTP ${response.status} on ${path}`;
    throw new ApiError(response.status, message, parsed);
  }

  return parsed as T;
}

export async function getMe(): Promise<ApiUser | null> {
  try {
    return await requestJson<ApiUser>('/v1/me', { method: 'GET' });
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      return null;
    }
    throw error;
  }
}

export function login(email: string, password: string): Promise<ApiUser> {
  return requestJson<ApiUser>('/v1/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
}

export function register(email: string, password: string): Promise<ApiUser> {
  return requestJson<ApiUser>('/v1/auth/register', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
}

export async function loginOrRegisterDemo(
  email: string,
  password: string,
): Promise<ApiUser> {
  try {
    return await login(email, password);
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      return register(email, password);
    }
    throw error;
  }
}

export function logout(): Promise<void> {
  return requestJson<void>('/v1/auth/logout', { method: 'POST' });
}

export function listCourses(): Promise<CourseSummary[]> {
  return requestJson<CourseSummary[]>('/v1/courses', { method: 'GET' });
}

export function enrollCourse(courseId: string): Promise<void> {
  return requestJson<void>(`/v1/courses/${encodeURIComponent(courseId)}/enroll`, {
    method: 'POST',
  });
}

export function getCourseFortress(courseId: string): Promise<CourseShieldDashboardApi> {
  return requestJson<CourseShieldDashboardApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/fortress`,
    { method: 'GET' },
  );
}

export function recomputeCourseShields(
  courseId: string,
): Promise<CourseShieldDashboardApi> {
  return requestJson<CourseShieldDashboardApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/shields/recompute`,
    { method: 'POST' },
  );
}

export function reportNodeContent(
  courseId: string,
  nodeId: string,
  reason = 'BROKEN_QUESTION',
  details = 'Reported from interactive shell',
  contentVersionId?: string | null,
): Promise<NodeReportOutcomeApi> {
  return requestJson<NodeReportOutcomeApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/nodes/${encodeURIComponent(nodeId)}/report`,
    {
      method: 'POST',
      body: JSON.stringify({
        reason,
        details,
        ...(contentVersionId ? { content_version_id: contentVersionId } : {}),
      }),
    },
  );
}

export function matchmakeDuel(
  courseId: string,
  preferMode?: DuelOpponentKind,
): Promise<MatchmakeDuelResponse> {
  return requestJson<MatchmakeDuelResponse>(
    `/v1/courses/${encodeURIComponent(courseId)}/duels/matchmake`,
    {
      method: 'POST',
      body: JSON.stringify({
        wait_budget_ms: 0,
        ...(preferMode ? { prefer_mode: preferMode } : {}),
      }),
    },
  );
}

export interface DuelSocketConnection {
  send(cmd: DuelClientCommand): void;
  close(): void;
  isOpen(): boolean;
}

export function connectDuelWebSocket(
  matchId: string,
  handlers: {
    onOpen?: () => void;
    onFrame: (frame: DuelServerFrame) => void;
    onClose?: (event: CloseEvent) => void;
    onError?: (error: Event) => void;
  },
): DuelSocketConnection {
  const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${wsProto}//${window.location.host}/v1/duels/ws?match_id=${encodeURIComponent(matchId)}`;
  const ws = new WebSocket(url);

  ws.addEventListener('open', () => {
    handlers.onOpen?.();
  });

  ws.addEventListener('message', event => {
    try {
      const frame = JSON.parse(String(event.data)) as DuelServerFrame;
      handlers.onFrame(frame);
    } catch {
      // Ignore malformed frames cleanly
    }
  });

  ws.addEventListener('close', event => {
    handlers.onClose?.(event);
  });

  ws.addEventListener('error', event => {
    handlers.onError?.(event);
  });

  return {
    send(cmd: DuelClientCommand) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(cmd));
      }
    },
    close() {
      if (
        ws.readyState === WebSocket.OPEN ||
        ws.readyState === WebSocket.CONNECTING
      ) {
        ws.close();
      }
    },
    isOpen() {
      return ws.readyState === WebSocket.OPEN;
    },
  };
}

export function setRaidOptIn(
  courseId: string,
  optIn: boolean,
): Promise<{ course_id: string; raid_opt_in: boolean }> {
  return requestJson<{ course_id: string; raid_opt_in: boolean }>(
    `/v1/courses/${encodeURIComponent(courseId)}/raids/opt-in`,
    {
      method: 'POST',
      body: JSON.stringify({ opt_in: optIn }),
    },
  );
}

export function scanRaidTargets(courseId: string): Promise<RaidScannerApi> {
  return requestJson<RaidScannerApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/raids/scanner`,
    { method: 'GET' },
  );
}

export async function startAndSettleRaid(
  courseId: string,
  input: {
    defenderUserId: string;
    defenderNodeId: string;
    isRevenge: boolean;
    revengeSourceRaidId?: string | null;
    sandboxSolved?: boolean;
    drillsPassed?: number;
    baseScorePoints?: number;
  },
): Promise<{ raid_id: string; settlement: SettledRaidApi }> {
  const started = await requestJson<{
    raid: { raid_id: string };
  }>(`/v1/courses/${encodeURIComponent(courseId)}/raids/start`, {
    method: 'POST',
    body: JSON.stringify({
      defender_user_id: input.defenderUserId,
      defender_node_id: input.defenderNodeId,
      is_revenge: input.isRevenge,
      ...(input.revengeSourceRaidId
        ? { revenge_source_raid_id: input.revengeSourceRaidId }
        : {}),
    }),
  });

  return requestJson<{ raid_id: string; settlement: SettledRaidApi }>(
    `/v1/courses/${encodeURIComponent(courseId)}/raids/${encodeURIComponent(started.raid.raid_id)}/settle`,
    {
      method: 'POST',
      body: JSON.stringify({
        sandbox_solved: input.sandboxSolved ?? true,
        drills_passed: input.drillsPassed ?? 3,
        base_score_points: input.baseScorePoints ?? 28,
      }),
    },
  );
}

export function getLeagueBracket(
  courseId: string,
  weekKey: string,
  tier = 1,
): Promise<LeagueBracketApi> {
  return requestJson<LeagueBracketApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/leagues/${encodeURIComponent(weekKey)}?tier=${tier}`,
    { method: 'GET' },
  );
}

export function rolloverLeague(
  courseId: string,
  weekKey: string,
  tier = 1,
): Promise<{
  outcome: {
    ranked_count?: number;
    max_promo_slots?: number;
  };
  standings: LeagueBracketApi['standings'];
}> {
  return requestJson(
    `/v1/courses/${encodeURIComponent(courseId)}/leagues/rollover`,
    {
      method: 'POST',
      body: JSON.stringify({ week_key: weekKey, tier }),
    },
  );
}

export function listBosses(
  courseId: string,
): Promise<{ bosses: ChapterBossApi[] }> {
  return requestJson<{ bosses: ChapterBossApi[] }>(
    `/v1/courses/${encodeURIComponent(courseId)}/bosses`,
    { method: 'GET' },
  );
}

export function strikeBossLive(
  courseId: string,
  bossId: string,
  input: {
    sequence: number;
    rating: number;
    overcharged: boolean;
    baseDamage: number;
  },
): Promise<BossStrikeResponseApi> {
  return requestJson<BossStrikeResponseApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/bosses/${encodeURIComponent(bossId)}/strike`,
    {
      method: 'POST',
      body: JSON.stringify({
        sequence: input.sequence,
        rating: input.rating,
        overcharged: input.overcharged,
        base_damage: input.baseDamage,
      }),
    },
  );
}

export function claimBossChestLive(
  courseId: string,
  bossId: string,
): Promise<BossChestClaimResponseApi> {
  return requestJson<BossChestClaimResponseApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/bosses/${encodeURIComponent(bossId)}/claim`,
    {
      method: 'POST',
      body: JSON.stringify({}),
    },
  );
}

export function getEconomy(courseId: string): Promise<EconomyOverviewApi> {
  return requestJson<EconomyOverviewApi>(
    `/v1/courses/${encodeURIComponent(courseId)}/economy`,
    { method: 'GET' },
  );
}
