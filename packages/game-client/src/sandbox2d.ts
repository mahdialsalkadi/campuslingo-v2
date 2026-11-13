import {
  THEME_PALETTES,
  type Archetype,
  type DirectorTrack,
  type FortressNodeModel,
  type FortressStateModel,
  type WorldTheme,
} from './catalog.js';

export interface Sandbox2DState {
  theme: WorldTheme;
  fortress: FortressStateModel;
  selectedNodeId: string;
  archetype: Archetype;
  modifier: string;
  parameterValues: Record<string, number>;
  targetReadings: readonly { name: string; value: number | null; passed: boolean; lower: number; upper: number }[];
  track: DirectorTrack | null;
  visible: boolean;
}

/**
 * Accessible 2D ConceptSandbox2D fallback (O-05) triggered either manually or after
 * sustained < 28 FPS for 3.0 seconds, encoding node state with color + shape + badge.
 */
export class ConceptSandbox2D {
  private readonly container: HTMLElement;
  private readonly onSelectNode: (nodeId: string) => void;
  private state: Sandbox2DState;

  constructor(
    container: HTMLElement,
    initialState: Sandbox2DState,
    onSelectNode: (nodeId: string) => void,
  ) {
    this.container = container;
    this.state = initialState;
    this.onSelectNode = onSelectNode;
    this.render();
  }

  updateState(next: Sandbox2DState): void {
    this.state = next;
    this.render();
  }

  private nodeGlyph(node: FortressNodeModel): string {
    if (node.state === 'INACTIVE' || node.tier === null) return '◇ INACTIVE';
    if (node.is_brownout) return `▲ TIER ${node.tier} BROWNOUT`;
    return `◆ TIER ${node.tier} ACTIVE`;
  }

  private render(): void {
    if (!this.state.visible) {
      this.container.replaceChildren();
      return;
    }
    const palette = THEME_PALETTES[this.state.theme];
    const width = 560;
    const height = 280;
    const scaleX = 76;
    const scaleY = 58;
    const ox = 250;
    const oy = 140;

    const edgesSvg = this.state.fortress.edges
      .map(edge => {
        const from = this.state.fortress.nodes.find(n => n.node_id === edge.prerequisite_node_id);
        const to = this.state.fortress.nodes.find(n => n.node_id === edge.dependent_node_id);
        if (!from || !to) return '';
        const x1 = ox + from.grid_x * scaleX;
        const y1 = oy + from.grid_y * scaleY;
        const x2 = ox + to.grid_x * scaleX;
        const y2 = oy + to.grid_y * scaleY;
        const degraded = from.is_brownout && !from.propagation_exempt;
        const color = degraded ? '#f97316' : palette.accentCss;
        return `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${color}" stroke-width="2.5" stroke-dasharray="${degraded ? '6 4' : 'none'}" marker-end="url(#arrow2d)" />`;
      })
      .join('');

    const cardsHtml = this.state.fortress.nodes
      .map(node => {
        const selected = node.node_id === this.state.selectedNodeId;
        const protBadge = node.raid_immune
          ? `<span class="badge-chip badge-safe">SHIELDED: ${node.protection_reasons[0] ?? 'PROTECTED'}</span>`
          : node.is_brownout
            ? '<span class="badge-chip badge-warn">BROWNOUT &lt; 7000</span>'
            : '<span class="badge-chip">NOMINAL</span>';
        return `<button type="button" class="fallback-node-btn ${selected ? 'is-selected' : ''}" data-node-id="${node.node_id}">
          <strong>${node.title}</strong>
          <span>${this.nodeGlyph(node)} · E=${node.e_u_ticks ?? '—'}</span>
          ${protBadge}
        </button>`;
      })
      .join('');

    const targetsHtml = this.state.targetReadings
      .map(t => `<div class="metric-bar ${t.passed ? 'ok' : 'warn'}">
        <span>${t.name}: ${t.value === null ? 'ERR' : t.value.toFixed(2)} [${t.lower}..${t.upper}]</span>
        <strong>${t.passed ? 'PASS' : 'ADJUST'}</strong>
      </div>`)
      .join('');

    this.container.innerHTML = `
      <div class="fallback-2d-shell" role="region" aria-label="2D Accessible Strategy Fallback">
        <div class="fallback-header">
          <span class="badge-chip">2D ACCESSIBLE FALLBACK · ${this.state.archetype} / ${this.state.modifier}</span>
          <span>Shot: ${this.state.track?.shot_type ?? 'overview'} · Explode: ${((this.state.track?.explode_factor ?? 0.1) * 100).toFixed(0)}%</span>
        </div>
        <svg viewBox="0 0 ${width} ${height}" class="fallback-dag-svg" aria-label="Prerequisite DAG Schematic">
          <defs>
            <marker id="arrow2d" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
              <path d="M 0 1 L 10 5 L 0 9 z" fill="${palette.accentCss}" />
            </marker>
          </defs>
          ${edgesSvg}
        </svg>
        <div class="fallback-node-grid">${cardsHtml}</div>
        <div class="fallback-schematic-metrics">${targetsHtml}</div>
      </div>
    `;

    this.container.querySelectorAll<HTMLButtonElement>('[data-node-id]').forEach(btn => {
      btn.addEventListener('click', () => {
        const id = btn.dataset.nodeId;
        if (id) this.onSelectNode(id);
      });
    });
  }
}
