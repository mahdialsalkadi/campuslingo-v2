import {
  connectDuelWebSocket,
  matchmakeDuel,
  type DuelSocketConnection,
} from './api.js';
import type { EvaluateJsonFn } from './controller.js';

export const TOAST_MAX_DURATION_MS = 2500;
export const DUEL_DURATION_MS = 90_000;

export type DuelOpponentKind = 'HUMAN' | 'GHOST' | 'BOT';

export interface DuelHandCardView {
  card_id: string;
  archetype: string;
  modifier: string;
  card_cost: number;
  base_window_ms: number;
  prompt?: string;
  options?: string[];
}

export interface DuelFighterProjection {
  energy_milli: number;
  tower_hp: [number, number];
  tower_shield: [number, number];
  core_hp: number;
  combat_level: number;
  overcharge_count: number;
  counter_strike_active: boolean;
  quick_blitz_chain_available?: boolean;
  void_transport_count?: number;
  too_fast_count?: number;
  last_sequence?: number;
  selected_card_id?: string | null;
  hand?: DuelHandCardView[];
}

export interface LiveDuelArenaProjection {
  protocol_version: number;
  viewer_side: 'PLAYER_A' | 'PLAYER_B';
  opponent_kind: DuelOpponentKind;
  match_clock_ms: number;
  sudden_death_elapsed_ms: number;
  remaining_ms: number;
  double_energy: boolean;
  status: 'IN_PROGRESS' | 'SUDDEN_DEATH' | 'COMPLETED';
  winner: 'PLAYER_A' | 'PLAYER_B' | null;
  self: DuelFighterProjection;
  opponent: DuelFighterProjection;
}

export interface HudToastState {
  visible: boolean;
  message: string;
  variant: 'info' | 'overcharge' | 'counter' | 'warn';
  durationMs: number;
}

export class HudToastController {
  private readonly host: HTMLElement;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private state: HudToastState = {
    visible: false,
    message: '',
    variant: 'info',
    durationMs: TOAST_MAX_DURATION_MS,
  };
  private onChange?: (state: HudToastState) => void;

  constructor(host: HTMLElement, onChange?: (state: HudToastState) => void) {
    this.host = host;
    this.onChange = onChange;
  }

  public show(
    message: string,
    variant: HudToastState['variant'] = 'info',
    requestedDurationMs = TOAST_MAX_DURATION_MS,
  ): HudToastState {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    const clampedMs = Math.max(400, Math.min(TOAST_MAX_DURATION_MS, requestedDurationMs));
    this.state = {
      visible: true,
      message,
      variant,
      durationMs: clampedMs,
    };
    this.render();
    this.onChange?.(this.state);
    this.timer = setTimeout(() => {
      this.dismiss();
    }, clampedMs);
    return this.state;
  }

  public dismiss(): HudToastState {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.state = { ...this.state, visible: false };
    this.host.replaceChildren();
    this.onChange?.(this.state);
    return this.state;
  }

  public getState(): HudToastState {
    return { ...this.state };
  }

  private render(): void {
    if (!this.state.visible) {
      this.host.replaceChildren();
      return;
    }
    const card = document.createElement('div');
    card.className = `m5-toast-card m5-toast-${this.state.variant}`;
    card.setAttribute('role', 'status');
    card.setAttribute('aria-live', 'polite');
    card.dataset.testid = 'm5-hud-toast';

    const text = document.createElement('span');
    text.className = 'm5-toast-text';
    text.textContent = `${this.state.message} (${(this.state.durationMs / 1000).toFixed(1)}s)`;

    const skipBtn = document.createElement('button');
    skipBtn.type = 'button';
    skipBtn.className = 'touch-btn m5-toast-skip';
    skipBtn.dataset.testid = 'm5-toast-skip';
    skipBtn.textContent = 'Skip';
    skipBtn.addEventListener('click', () => {
      this.dismiss();
    });

    card.append(text, skipBtn);
    this.host.replaceChildren(card);
  }
}

export function createInitialDuelProjection(
  opponentKind: DuelOpponentKind = 'GHOST',
): LiveDuelArenaProjection {
  return {
    protocol_version: 2,
    viewer_side: 'PLAYER_A',
    opponent_kind: opponentKind,
    match_clock_ms: 0,
    sudden_death_elapsed_ms: 0,
    remaining_ms: DUEL_DURATION_MS,
    double_energy: false,
    status: 'IN_PROGRESS',
    winner: null,
    self: {
      energy_milli: 5000,
      tower_hp: [1800, 1800],
      tower_shield: [300, 300],
      core_hp: 3000,
      combat_level: 11,
      overcharge_count: 0,
      counter_strike_active: false,
      quick_blitz_chain_available: true,
      void_transport_count: 0,
      too_fast_count: 0,
      last_sequence: 0,
      selected_card_id: 'a-card-0',
      hand: [
        { card_id: 'a-card-0', archetype: 'AXIOM', modifier: 'COROLLARY_CHAIN', card_cost: 2, base_window_ms: 8000, prompt: 'Direct Overcharge Strike' },
        { card_id: 'a-card-1', archetype: 'PIPELINE', modifier: 'FAULT_INJECTION', card_cost: 3, base_window_ms: 9000, prompt: 'Pipeline Bottleneck Repair' },
        { card_id: 'a-card-2', archetype: 'SPATIAL', modifier: 'CROSS_SECTION_SCAN', card_cost: 3, base_window_ms: 9000, prompt: 'Cross-Section Ward' },
        { card_id: 'a-card-3', archetype: 'AXIOM', modifier: 'SPEED_BLITZ', card_cost: 1, base_window_ms: 6000, prompt: 'Speed Blitz Pulse' },
      ],
    },
    opponent: {
      energy_milli: 5000,
      tower_hp: [1800, 1800],
      tower_shield: [300, 300],
      core_hp: 3000,
      combat_level: 11,
      overcharge_count: 0,
      counter_strike_active: false,
    },
  };
}

export function formatDuelClock(remainingMs: number): string {
  const secs = Math.max(0, Math.ceil(remainingMs / 1000));
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export interface DuelPanelController {
  setLiveCourse(courseId: string | null): void;
  startLiveMatch(): Promise<void>;
  updateProjection(next: LiveDuelArenaProjection): void;
  getProjection(): LiveDuelArenaProjection;
  getToastState(): HudToastState;
  destroy(): void;
}

export function mountDuelArenaPanel(
  container: HTMLElement,
  evaluateJson: EvaluateJsonFn,
): DuelPanelController {
  let state = createInitialDuelProjection('GHOST');
  let activeLane: 0 | 1 = 0;
  let liveCourseId: string | null = null;
  let liveMatchId: string | null = null;
  let liveSocket: DuelSocketConnection | null = null;
  let clientSeq = 0;
  const nextClientSeq = () => {
    clientSeq += 1;
    return clientSeq;
  };

  container.innerHTML = `
    <div class="m5-duel-panel" data-testid="m5-duel-panel">
      <div class="m4-subhead">
        <h3>M5 Live 90s 2-Lane Duel Arena</h3>
        <div class="m5-duel-badges">
          <span class="badge-chip badge-safe" id="m5-opp-kind" data-testid="m5-opp-kind">GHOST</span>
          <span class="badge-chip" id="m5-clock-badge" data-testid="m5-clock-badge">01:30</span>
          <span class="badge-chip" id="m5-energy-mode" data-testid="m5-energy-mode">1x REGEN</span>
        </div>
      </div>

      <div class="m5-toast-layer" id="m5-toast-layer" data-testid="m5-toast-layer"></div>

      <div class="m5-lanes-grid" id="m5-lanes-grid"></div>

      <div class="m5-energy-row">
        <span id="m5-energy-readout" data-testid="m5-energy-readout">Energy: 5.0 / 10</span>
        <span id="m5-status-readout" class="m4-muted-note" data-testid="m5-status-readout">L11 Tournament Parity · 0 Overcharges</span>
      </div>

      <div class="m5-hand-row" id="m5-hand-row" data-testid="m5-hand-row"></div>

      <div class="m5-duel-actions">
        <button type="button" class="touch-btn" id="m5-live-matchmake" data-testid="m5-live-matchmake">Live WS Matchmake</button>
        <button type="button" class="touch-btn accent-btn" id="m5-strike-overcharge" data-testid="m5-strike-overcharge">Strike 2.0x Overcharge</button>
        <button type="button" class="touch-btn" id="m5-advance-30s" data-testid="m5-advance-30s">+30s Final 2x Phase</button>
        <button type="button" class="touch-btn" id="m5-toast-demo" data-testid="m5-toast-demo">Show &lt;=2.5s HUD Toast</button>
      </div>
    </div>
  `;

  const oppKindEl = container.querySelector('#m5-opp-kind') as HTMLElement;
  const clockBadgeEl = container.querySelector('#m5-clock-badge') as HTMLElement;
  const energyModeEl = container.querySelector('#m5-energy-mode') as HTMLElement;
  const toastLayerEl = container.querySelector('#m5-toast-layer') as HTMLElement;
  const lanesGridEl = container.querySelector('#m5-lanes-grid') as HTMLElement;
  const energyReadoutEl = container.querySelector('#m5-energy-readout') as HTMLElement;
  const statusReadoutEl = container.querySelector('#m5-status-readout') as HTMLElement;
  const handRowEl = container.querySelector('#m5-hand-row') as HTMLElement;
  const liveMatchBtn = container.querySelector('#m5-live-matchmake') as HTMLButtonElement;
  const strikeBtn = container.querySelector('#m5-strike-overcharge') as HTMLButtonElement;
  const advanceBtn = container.querySelector('#m5-advance-30s') as HTMLButtonElement;
  const toastDemoBtn = container.querySelector('#m5-toast-demo') as HTMLButtonElement;

  const toast = new HudToastController(toastLayerEl);

  const render = () => {
    oppKindEl.textContent = `${state.opponent_kind} · L${state.self.combat_level}`;
    clockBadgeEl.textContent =
      state.status === 'COMPLETED'
        ? `DONE (${state.winner ?? 'DRAW'})`
        : state.status === 'SUDDEN_DEATH'
          ? `SD ${formatDuelClock(state.remaining_ms)}`
          : formatDuelClock(state.remaining_ms);
    energyModeEl.textContent = state.double_energy ? '2x ENERGY' : '1x REGEN';
    energyModeEl.className = state.double_energy ? 'badge-chip badge-warn' : 'badge-chip';

    energyReadoutEl.textContent = `Energy: ${(state.self.energy_milli / 1000).toFixed(1)} / 10`;
    statusReadoutEl.textContent = `Overcharges: ${state.self.overcharge_count} · Core ${state.self.core_hp} vs ${state.opponent.core_hp}`;

    lanesGridEl.replaceChildren(
      ...[0, 1].map(idx => {
        const laneBtn = document.createElement('button');
        laneBtn.type = 'button';
        laneBtn.className = 'touch-btn m5-lane-card';
        laneBtn.dataset.testid = `m5-lane-${idx}`;
        laneBtn.setAttribute('aria-pressed', activeLane === idx ? 'true' : 'false');
        laneBtn.textContent = `Lane ${idx + 1} · Opp ${state.opponent.tower_hp[idx]} HP (Sh ${state.opponent.tower_shield[idx]}) · You ${state.self.tower_hp[idx]} HP`;
        laneBtn.addEventListener('click', () => {
          activeLane = idx as 0 | 1;
          render();
        });
        return laneBtn;
      }),
    );

    const cards = state.self.hand ?? [];
    handRowEl.replaceChildren(
      ...cards.map(card => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn m5-hand-card';
        btn.dataset.testid = `m5-card-${card.card_id}`;
        btn.setAttribute(
          'aria-pressed',
          state.self.selected_card_id === card.card_id ? 'true' : 'false',
        );
        btn.textContent = `[${card.card_cost}] ${card.archetype} (${card.base_window_ms / 1000}s)`;
        btn.addEventListener('click', () => {
          state = {
            ...state,
            self: { ...state.self, selected_card_id: card.card_id },
          };
          if (liveSocket?.isOpen()) {
            liveSocket.send({
              type: 'SELECT_CARD',
              sequence: nextClientSeq(),
              card_id: card.card_id,
            });
          }
          render();
        });
        return btn;
      }),
    );
  };

  const startLiveMatch = async () => {
    if (!liveCourseId) {
      toast.show('Connect to live server first', 'warn', 2000);
      return;
    }
    try {
      liveMatchBtn.textContent = 'Matching...';
      const resp = await matchmakeDuel(liveCourseId);
      liveSocket?.close();
      liveMatchId = resp.match_id;
      state = resp.arena;
      clientSeq = state.self.last_sequence ?? 0;
      render();

      liveSocket = connectDuelWebSocket(resp.match_id, {
        onOpen: () => {
          liveSocket?.send({
            type: 'START_ROUND',
            sequence: nextClientSeq(),
          });
          toast.show(
            `Live WS match ready (${resp.opponent_kind})`,
            'info',
            2200,
          );
        },
        onFrame: frame => {
          if (
            (frame.type === 'MATCH_STATE' || frame.type === 'SNAPSHOT_RESYNC') &&
            frame.payload
          ) {
            state = frame.payload as LiveDuelArenaProjection;
            render();
          } else if (frame.type === 'STRIKE_RESOLVED' && frame.payload) {
            const payload = frame.payload as {
              strike?: {
                result?: { damage_to_tower?: number; overcharged?: boolean };
              };
              arena?: LiveDuelArenaProjection;
            };
            if (payload.arena) {
              state = payload.arena;
            }
            const dmg = payload.strike?.result?.damage_to_tower ?? 0;
            const overcharged = Boolean(payload.strike?.result?.overcharged);
            toast.show(
              overcharged
                ? `WS Overcharge! -${dmg} Tower HP`
                : `WS Strike: -${dmg} Tower HP`,
              overcharged ? 'overcharge' : 'info',
              2200,
            );
            render();
          } else if (frame.type === 'ERROR') {
            const errPayload = frame as { code?: string };
            toast.show(`WS Error: ${errPayload.code ?? 'UNKNOWN'}`, 'warn', 2200);
          }
        },
        onClose: () => {
          if (liveMatchId === resp.match_id) {
            liveMatchBtn.textContent = 'Live WS Matchmake';
          }
        },
      });
      liveMatchBtn.textContent = 'Rematch WS';
    } catch (err) {
      liveMatchBtn.textContent = 'Live WS Matchmake';
      toast.show(
        err instanceof Error ? err.message : 'Live matchmake failed',
        'warn',
        2200,
      );
    }
  };

  liveMatchBtn.addEventListener('click', () => {
    void startLiveMatch();
  });

  strikeBtn.addEventListener('click', () => {
    const cards = state.self.hand ?? [];
    const chosen =
      cards.find(c => c.card_id === state.self.selected_card_id) ?? cards[0];
    if (!chosen) return;
    const nextSeq = (state.self.last_sequence ?? 0) + 1;

    if (liveMatchId && liveSocket?.isOpen()) {
      const seq = Math.max(nextSeq, nextClientSeq());
      clientSeq = seq;
      liveSocket.send({ type: 'START_ROUND', sequence: seq });
      liveSocket.send({
        type: 'SELECT_CARD',
        sequence: seq,
        card_id: chosen.card_id,
      });
      liveSocket.send({
        type: 'STRIKE',
        sequence: seq,
        lane: activeLane,
        card_id: chosen.card_id,
        selected_index: 0,
        use_quick_blitz_chain: false,
        client_elapsed_ms: Math.max(400, Math.floor(chosen.base_window_ms * 0.4)),
      });
      return;
    }
    const fullArenaReq = {
      kind: 'duel_strike',
      arena: {
        protocol_version: 2,
        match_clock_ms: state.match_clock_ms,
        sudden_death_elapsed_ms: state.sudden_death_elapsed_ms,
        status: state.status,
        winner: state.winner,
        player_a: {
          side: 'PLAYER_A',
          energy_milli: state.self.energy_milli,
          tower_hp: state.self.tower_hp,
          tower_shield: state.self.tower_shield,
          core_hp: state.self.core_hp,
          accommodation_num: 1,
          accommodation_den: 1,
          combat_level: state.self.combat_level,
          void_transport_count: state.self.void_transport_count ?? 0,
          too_fast_count: state.self.too_fast_count ?? 0,
          overcharge_count: state.self.overcharge_count,
          counter_strike_until_ms: null,
          quick_blitz_chain_available: false,
          last_sequence: state.self.last_sequence ?? 0,
          hand: cards,
          selected_card_id: chosen.card_id,
          selection_started_at_ms: null,
        },
        player_b: {
          side: 'PLAYER_B',
          energy_milli: state.opponent.energy_milli,
          tower_hp: state.opponent.tower_hp,
          tower_shield: state.opponent.tower_shield,
          core_hp: state.opponent.core_hp,
          accommodation_num: 1,
          accommodation_den: 1,
          combat_level: state.opponent.combat_level,
          void_transport_count: 0,
          too_fast_count: 0,
          overcharge_count: state.opponent.overcharge_count,
          counter_strike_until_ms: null,
          quick_blitz_chain_available: false,
          last_sequence: 0,
          hand: cards.map(c => ({ ...c, card_id: `opp-${c.card_id}` })),
          selected_card_id: null,
          selection_started_at_ms: null,
        },
      },
      strike: {
        attacker: 'PLAYER_A',
        sequence: nextSeq,
        lane: activeLane,
        card_id: chosen.card_id,
        use_quick_blitz_chain: false,
        termination: 'SUBMITTED',
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: Math.floor(chosen.base_window_ms * 0.4),
        rtt_compensation_ms: 0,
        opponent_card_base_ms: chosen.base_window_ms,
      },
    };
    try {
      const resp = JSON.parse(evaluateJson(JSON.stringify(fullArenaReq))) as {
        kind: string;
        arena?: {
          match_clock_ms: number;
          status: LiveDuelArenaProjection['status'];
          winner: LiveDuelArenaProjection['winner'];
          player_a: DuelFighterProjection & { hand: DuelHandCardView[]; last_sequence: number };
          player_b: DuelFighterProjection;
        };
        result?: { damage_to_tower: number; overcharged: boolean };
      };
      if (resp.kind === 'duel_strike' && resp.arena && resp.result) {
        state = {
          ...state,
          match_clock_ms: resp.arena.match_clock_ms,
          status: resp.arena.status,
          winner: resp.arena.winner,
          self: {
            ...state.self,
            energy_milli: Math.min(10000, resp.arena.player_a.energy_milli + 2000),
            overcharge_count: resp.arena.player_a.overcharge_count,
            last_sequence: resp.arena.player_a.last_sequence,
          },
          opponent: {
            ...state.opponent,
            tower_hp: resp.arena.player_b.tower_hp,
            tower_shield: resp.arena.player_b.tower_shield,
            core_hp: resp.arena.player_b.core_hp,
          },
        };
        toast.show(
          resp.result.overcharged
            ? `2.0x Overcharge Shield-Pierce! -${resp.result.damage_to_tower} Tower HP on Lane ${activeLane + 1}`
            : `Strike landed: -${resp.result.damage_to_tower} Tower HP`,
          resp.result.overcharged ? 'overcharge' : 'info',
          2200,
        );
        render();
      }
    } catch {
      toast.show('Duel WASM preview unavailable', 'warn', 1800);
    }
  });

  advanceBtn.addEventListener('click', () => {
    if (liveMatchId && liveSocket?.isOpen()) {
      liveSocket.send({ type: 'TICK_ADVANCE', delta_ms: 30_000 });
      return;
    }
    const nextClock = Math.min(DUEL_DURATION_MS, state.match_clock_ms + 30_000);
    const remaining = Math.max(0, DUEL_DURATION_MS - nextClock);
    const doubleEnergy = nextClock >= 60_000;
    state = {
      ...state,
      match_clock_ms: nextClock,
      remaining_ms: remaining,
      double_energy: doubleEnergy,
      self: { ...state.self, energy_milli: Math.min(10000, state.self.energy_milli + 4000) },
      opponent: { ...state.opponent, energy_milli: Math.min(10000, state.opponent.energy_milli + 4000) },
    };
    toast.show(
      doubleEnergy ? 'Final 30s: 2x Energy Regeneration active!' : `Clock advanced to ${formatDuelClock(remaining)}`,
      doubleEnergy ? 'counter' : 'info',
      2000,
    );
    render();
  });

  toastDemoBtn.addEventListener('click', () => {
    toast.show('Non-blocking HUD Alert: 2x Energy & Counter-Strike ready', 'overcharge', 2500);
  });

  render();

  return {
    setLiveCourse(courseId) {
      if (liveCourseId !== courseId) {
        liveSocket?.close();
        liveSocket = null;
        liveMatchId = null;
        liveMatchBtn.textContent = 'Live WS Matchmake';
      }
      liveCourseId = courseId;
    },
    startLiveMatch,
    updateProjection(next) {
      state = next;
      render();
    },
    getProjection() {
      return state;
    },
    getToastState() {
      return toast.getState();
    },
    destroy() {
      liveSocket?.close();
      liveSocket = null;
      toast.dismiss();
      container.replaceChildren();
    },
  };
}
