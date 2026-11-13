import * as THREE from 'three';
import {
  THEME_PALETTES,
  type Archetype,
  type DirectorTrack,
  type FortressNodeModel,
  type FortressStateModel,
  type ThemePalette,
  type WorldTheme,
} from './catalog.js';
import {
  FpsHysteresisMonitor,
  validateAssetBudget,
  type BudgetValidationResult,
  type FrameMetricsSnapshot,
} from './validator.js';

export interface SceneRenderState {
  theme: WorldTheme;
  fortress: FortressStateModel;
  selectedNodeId: string;
  archetype: Archetype;
  modifier: string;
  parameterValues: Record<string, number>;
  evaluationAllPassed: boolean;
  track: DirectorTrack | null;
  cameraFocusMode: 'hologram' | 'fortress';
  reducedMotion: boolean;
}

export interface Scene3DTelemetry {
  frame: FrameMetricsSnapshot;
  budget: BudgetValidationResult;
  unitTriangleMax: number;
  rendererBackend: 'webgl' | 'fallback-noop';
  cameraTarget: [number, number, number];
  activeExplodeFactor: number;
}

const FOCUS_ANCHOR_POS: Record<NonNullable<SceneRenderState['track']>['focus_role'], THREE.Vector3> = {
  concept: new THREE.Vector3(0, 2.25, 0),
  input: new THREE.Vector3(-0.95, 2.2, 0.2),
  output: new THREE.Vector3(0.95, 2.2, 0.2),
  fault: new THREE.Vector3(0, 2.0, 0.55),
  comparison: new THREE.Vector3(0, 2.35, 0),
};

function geometryTriangles(geo: THREE.BufferGeometry): number {
  const idx = geo.getIndex();
  if (idx) return Math.floor(idx.count / 3);
  const pos = geo.getAttribute('position');
  return pos ? Math.floor(pos.count / 3) : 0;
}

function groupTriangles(root: THREE.Object3D): number {
  let tris = 0;
  root.traverse(obj => {
    const mesh = obj as THREE.Mesh;
    if ((mesh.isMesh || obj.type === 'InstancedMesh') && mesh.geometry) {
      const base = geometryTriangles(mesh.geometry);
      const count = obj.type === 'InstancedMesh' ? (obj as THREE.InstancedMesh).count : 1;
      tris += base * count;
    }
  });
  return tris;
}

function disposeGroup(root: THREE.Object3D): void {
  root.traverse(obj => {
    const mesh = obj as THREE.Mesh;
    if (mesh.geometry) mesh.geometry.dispose();
    if (mesh.material) {
      if (Array.isArray(mesh.material)) mesh.material.forEach(m => m.dispose());
      else mesh.material.dispose();
    }
  });
  root.clear();
}

export class Fortress3DScene {
  private readonly container: HTMLElement;
  private readonly onSelectNode: (nodeId: string) => void;
  private readonly onMetrics: (telemetry: Scene3DTelemetry) => void;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly renderer: THREE.WebGLRenderer | null;
  private readonly rendererBackend: 'webgl' | 'fallback-noop';
  private readonly fortressGroup = new THREE.Group();
  private readonly hologramGroup = new THREE.Group();
  private readonly hudGridGroup = new THREE.Group();
  private readonly ambientLight = new THREE.AmbientLight(0xffffff, 0.9);
  private readonly keyLight = new THREE.DirectionalLight(0xffffff, 1.45);
  private readonly rimLight = new THREE.DirectionalLight(0x38bdf8, 0.85);
  private readonly monitor = new FpsHysteresisMonitor();
  private readonly raycaster = new THREE.Raycaster();
  private readonly nodeHitMeshes: THREE.Mesh[] = [];
  private readonly hologramParts: THREE.Object3D[] = [];

  private state: SceneRenderState;
  private currentCamPos = new THREE.Vector3(6.4, 5.4, 7.6);
  private currentTarget = new THREE.Vector3(0, 1.4, 0);
  private animId: number | null = null;
  private paused = false;
  private lastTickMs = 0;
  private lastBuildKey = '';
  private lastHologramKey = '';
  private maxUnitTriangles = 0;
  private totalSceneTriangles = 0;

  constructor(
    container: HTMLElement,
    initialState: SceneRenderState,
    callbacks: {
      onSelectNode: (nodeId: string) => void;
      onMetrics: (telemetry: Scene3DTelemetry) => void;
    },
  ) {
    this.container = container;
    this.state = initialState;
    this.onSelectNode = callbacks.onSelectNode;
    this.onMetrics = callbacks.onMetrics;

    const width = Math.max(300, container.clientWidth || 640);
    const height = Math.max(220, container.clientHeight || 420);
    this.camera = new THREE.PerspectiveCamera(40, width / height, 0.1, 80);
    this.camera.position.copy(this.currentCamPos);
    this.camera.lookAt(this.currentTarget);

    let renderer: THREE.WebGLRenderer | null = null;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
      renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
      renderer.setSize(width, height, false);
      renderer.domElement.className = 'fortress-webgl-canvas';
      renderer.domElement.setAttribute('aria-label', '3D Concept Fortress and Hologram View');
      container.replaceChildren(renderer.domElement);
      renderer.domElement.addEventListener('pointerdown', this.handlePointerDown);
    } catch {
      renderer = null;
    }

    this.renderer = renderer;
    this.rendererBackend = renderer ? 'webgl' : 'fallback-noop';

    this.keyLight.position.set(8, 12, 6);
    this.rimLight.position.set(-7, 6, -8);
    this.scene.add(this.ambientLight, this.keyLight, this.rimLight);
    this.scene.add(this.hudGridGroup, this.fortressGroup, this.hologramGroup);

    this.rebuildEnvironment(THEME_PALETTES[initialState.theme]);
    this.rebuildFortress();
    this.rebuildHologram();
    this.startLoop();
  }

  private readonly handlePointerDown = (event: PointerEvent): void => {
    if (!this.renderer || this.nodeHitMeshes.length === 0) return;
    const rect = this.renderer.domElement.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / Math.max(1, rect.width)) * 2 - 1;
    const y = -((event.clientY - rect.top) / Math.max(1, rect.height)) * 2 + 1;
    this.raycaster.setFromCamera(new THREE.Vector2(x, y), this.camera);
    const hits = this.raycaster.intersectObjects(this.nodeHitMeshes, false);
    const hitNodeId = hits[0]?.object.userData?.nodeId;
    if (typeof hitNodeId === 'string') {
      this.onSelectNode(hitNodeId);
    }
  };

  setForcedLowFps(enabled: boolean): void {
    this.monitor.setForcedLowFps(enabled);
  }

  resetFallbackMonitor(): void {
    this.monitor.reset();
  }

  setPaused(paused: boolean): void {
    this.paused = paused;
    if (!paused && this.animId === null) {
      this.lastTickMs = performance.now();
      this.startLoop();
    }
  }

  resize(): void {
    if (!this.renderer) return;
    const width = Math.max(300, this.container.clientWidth || 640);
    const height = Math.max(220, this.container.clientHeight || 420);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height, false);
  }

  updateState(next: SceneRenderState): void {
    const themeChanged = next.theme !== this.state.theme;
    const fortressKey = `${next.theme}|${next.selectedNodeId}|${next.fortress.nodes.map(n => `${n.node_id}:${n.tier}:${n.is_brownout}:${n.raid_immune}`).join(',')}`;
    const hologramKey = `${next.theme}|${next.archetype}|${next.modifier}|${next.selectedNodeId}`;
    this.state = next;

    if (themeChanged) {
      this.rebuildEnvironment(THEME_PALETTES[next.theme]);
    }
    if (fortressKey !== this.lastBuildKey) {
      this.lastBuildKey = fortressKey;
      this.rebuildFortress();
    }
    if (hologramKey !== this.lastHologramKey) {
      this.lastHologramKey = hologramKey;
      this.rebuildHologram();
    }
    this.applyHologramDeformation(performance.now());
  }

  private rebuildEnvironment(palette: ThemePalette): void {
    this.scene.background = new THREE.Color(palette.backgroundHex);
    this.scene.fog = new THREE.FogExp2(palette.fogHex, 0.036);
    this.rimLight.color.setHex(palette.accentHex);

    disposeGroup(this.hudGridGroup);
    const grid = new THREE.GridHelper(14, 14, palette.gridPrimaryHex, palette.gridSecondaryHex);
    grid.position.y = -0.02;
    this.hudGridGroup.add(grid);

    const floorGeo = new THREE.PlaneGeometry(14, 14);
    const floorMat = new THREE.MeshStandardMaterial({
      color: palette.plinthHex,
      roughness: 0.9,
      metalness: 0.15,
    });
    const floor = new THREE.Mesh(floorGeo, floorMat);
    floor.rotation.x = -Math.PI / 2;
    floor.position.y = -0.04;
    this.hudGridGroup.add(floor);
  }

  private createNodePosition(node: FortressNodeModel): THREE.Vector3 {
    return new THREE.Vector3(node.grid_x * 1.65, 0, node.grid_y * 1.65);
  }

  private rebuildFortress(): void {
    disposeGroup(this.fortressGroup);
    this.nodeHitMeshes.length = 0;
    const palette = THEME_PALETTES[this.state.theme];
    const nodePositions = new Map<string, THREE.Vector3>();
    let maxUnitTris = 0;

    for (const node of this.state.fortress.nodes) {
      const pos = this.createNodePosition(node);
      nodePositions.set(node.node_id, pos);
      const unitGroup = new THREE.Group();
      unitGroup.position.copy(pos);

      const isSelected = node.node_id === this.state.selectedNodeId;
      const bodyColor =
        node.state === 'INACTIVE'
          ? palette.inactiveHex
          : node.is_brownout
            ? palette.brownoutHex
            : palette.healthyHex;

      const baseGeo = new THREE.CylinderGeometry(0.62, 0.72, 0.22, 8);
      const baseMat = new THREE.MeshStandardMaterial({
        color: isSelected ? palette.accentHex : palette.plinthHex,
        emissive: isSelected ? palette.accentHex : bodyColor,
        emissiveIntensity: isSelected ? 0.35 : 0.12,
        roughness: 0.45,
        metalness: 0.35,
      });
      const baseMesh = new THREE.Mesh(baseGeo, baseMat);
      baseMesh.position.y = 0.11;
      baseMesh.userData = { nodeId: node.node_id };
      this.nodeHitMeshes.push(baseMesh);
      unitGroup.add(baseMesh);

      const coreMat = new THREE.MeshStandardMaterial({
        color: bodyColor,
        emissive: bodyColor,
        emissiveIntensity: node.is_brownout ? 0.5 : 0.28,
        roughness: 0.3,
        metalness: 0.5,
      });

      const tier = node.tier ?? -1;
      if (tier < 0) {
        const wireframe = new THREE.Mesh(
          new THREE.OctahedronGeometry(0.28, 0),
          new THREE.MeshBasicMaterial({ color: palette.inactiveHex, wireframe: true }),
        );
        wireframe.position.y = 0.55;
        unitGroup.add(wireframe);
      } else {
        const core = new THREE.Mesh(new THREE.CylinderGeometry(0.22, 0.3, 0.45 + tier * 0.14, 6), coreMat);
        core.position.y = 0.44 + tier * 0.07;
        unitGroup.add(core);

        if (tier >= 1) {
          for (let i = 0; i < 4; i += 1) {
            const angle = (i * Math.PI) / 2 + Math.PI / 4;
            const pillar = new THREE.Mesh(new THREE.BoxGeometry(0.11, 0.52, 0.11), coreMat);
            pillar.position.set(Math.cos(angle) * 0.42, 0.45, Math.sin(angle) * 0.42);
            unitGroup.add(pillar);
          }
        }
        if (tier >= 2) {
          const ring = new THREE.Mesh(new THREE.TorusGeometry(0.46, 0.045, 6, 16), coreMat);
          ring.rotation.x = Math.PI / 2;
          ring.position.y = 0.78;
          unitGroup.add(ring);
        }
        if (tier >= 3) {
          const spire = new THREE.Mesh(new THREE.OctahedronGeometry(0.22, 0), coreMat);
          spire.position.y = 1.12;
          unitGroup.add(spire);
        }
      }

      if (node.raid_immune) {
        const dome = new THREE.Mesh(
          new THREE.IcosahedronGeometry(0.76, 1),
          new THREE.MeshBasicMaterial({
            color: palette.shieldDomeHex,
            wireframe: true,
            transparent: true,
            opacity: 0.48,
          }),
        );
        dome.position.y = 0.5;
        unitGroup.add(dome);
      }

      maxUnitTris = Math.max(maxUnitTris, groupTriangles(unitGroup));
      this.fortressGroup.add(unitGroup);
    }

    for (const edge of this.state.fortress.edges) {
      const from = nodePositions.get(edge.prerequisite_node_id);
      const to = nodePositions.get(edge.dependent_node_id);
      if (!from || !to) continue;
      const parent = this.state.fortress.nodes.find(n => n.node_id === edge.prerequisite_node_id);
      const degraded = Boolean(parent?.is_brownout && !parent.propagation_exempt);
      const delta = to.clone().sub(from);
      const dist = Math.max(0.2, delta.length());
      const beam = new THREE.Mesh(
        new THREE.BoxGeometry(0.08, 0.05, dist),
        new THREE.MeshBasicMaterial({ color: degraded ? palette.brownoutHex : palette.accentHex }),
      );
      beam.position.copy(from).lerp(to, 0.5).setY(0.16);
      beam.lookAt(to.x, 0.16, to.z);
      this.fortressGroup.add(beam);
    }

    this.maxUnitTriangles = maxUnitTris;
    this.recomputeSceneTriangles();
  }

  private rebuildHologram(): void {
    disposeGroup(this.hologramGroup);
    this.hologramParts.length = 0;

    const palette = THEME_PALETTES[this.state.theme];
    const selected = this.state.fortress.nodes.find(n => n.node_id === this.state.selectedNodeId)
      ?? this.state.fortress.nodes[0];
    const anchor = selected ? this.createNodePosition(selected) : new THREE.Vector3(0, 0, 0);
    this.hologramGroup.position.set(anchor.x, 1.75, anchor.z);

    const holoMat = () =>
      new THREE.MeshStandardMaterial({
        color: this.state.evaluationAllPassed ? palette.healthyHex : palette.accentHex,
        emissive: this.state.evaluationAllPassed ? palette.healthyHex : palette.accentHex,
        emissiveIntensity: 0.4,
        roughness: 0.25,
        metalness: 0.45,
        transparent: true,
        opacity: 0.9,
      });

    const faultMat = new THREE.MeshStandardMaterial({
      color: palette.brownoutHex,
      emissive: palette.brownoutHex,
      emissiveIntensity: 0.45,
      roughness: 0.35,
      metalness: 0.35,
    });

    switch (this.state.archetype) {
      case 'PIPELINE': {
        for (let i = -1; i <= 1; i += 1) {
          const chamber = new THREE.Mesh(new THREE.CylinderGeometry(0.24, 0.24, 0.42, 10), holoMat());
          chamber.rotation.z = Math.PI / 2;
          chamber.position.set(i * 0.72, 0.35, 0);
          chamber.userData = { baseX: i * 0.72, baseY: 0.35, role: 'pipeline_stage', idx: i + 1 };
          this.hologramParts.push(chamber);
          this.hologramGroup.add(chamber);
        }
        const conduit = new THREE.Mesh(new THREE.BoxGeometry(1.8, 0.1, 0.1), holoMat());
        conduit.position.set(0, 0.35, 0);
        conduit.userData = { baseY: 0.35, role: 'pipeline_flux' };
        this.hologramParts.push(conduit);
        this.hologramGroup.add(conduit);
        break;
      }
      case 'TRADEOFF': {
        const fulcrum = new THREE.Mesh(new THREE.ConeGeometry(0.25, 0.45, 6), holoMat());
        fulcrum.position.set(0, 0.18, 0);
        this.hologramGroup.add(fulcrum);
        const beam = new THREE.Mesh(new THREE.BoxGeometry(1.75, 0.1, 0.2), holoMat());
        beam.position.set(0, 0.48, 0);
        beam.userData = { baseY: 0.48, role: 'tradeoff_beam' };
        this.hologramParts.push(beam);
        this.hologramGroup.add(beam);
        const leftPan = new THREE.Mesh(new THREE.BoxGeometry(0.32, 0.32, 0.32), holoMat());
        leftPan.position.set(-0.78, 0.56, 0);
        const rightPan = new THREE.Mesh(new THREE.BoxGeometry(0.32, 0.32, 0.32), faultMat);
        rightPan.position.set(0.78, 0.56, 0);
        leftPan.userData = { baseX: -0.78, baseY: 0.56, role: 'tradeoff_left' };
        rightPan.userData = { baseX: 0.78, baseY: 0.56, role: 'tradeoff_right' };
        this.hologramParts.push(leftPan, rightPan);
        this.hologramGroup.add(leftPan, rightPan);
        break;
      }
      case 'SPATIAL': {
        for (let layer = 0; layer < 4; layer += 1) {
          const ring = new THREE.Mesh(
            new THREE.TorusGeometry(0.55 - layer * 0.09, 0.045, 8, 18),
            layer === 2 ? faultMat : holoMat(),
          );
          ring.rotation.x = Math.PI / 2;
          ring.position.set(0, 0.15 + layer * 0.14, 0);
          ring.userData = { baseY: 0.15 + layer * 0.14, layer, role: 'spatial_layer' };
          this.hologramParts.push(ring);
          this.hologramGroup.add(ring);
        }
        break;
      }
      case 'CONTRAST': {
        const colA = new THREE.Mesh(new THREE.BoxGeometry(0.38, 0.75, 0.38), holoMat());
        colA.position.set(-0.56, 0.45, 0);
        colA.userData = { baseX: -0.56, baseY: 0.45, role: 'contrast_a' };
        const colB = new THREE.Mesh(new THREE.BoxGeometry(0.38, 0.55, 0.38), faultMat);
        colB.position.set(0.56, 0.35, 0);
        colB.userData = { baseX: 0.56, baseY: 0.35, role: 'contrast_b' };
        this.hologramParts.push(colA, colB);
        this.hologramGroup.add(colA, colB);
        break;
      }
      case 'TRAP': {
        const shell = new THREE.Mesh(
          new THREE.BoxGeometry(0.95, 0.65, 0.95),
          new THREE.MeshBasicMaterial({ color: palette.accentHex, wireframe: true }),
        );
        shell.position.set(0, 0.42, 0);
        shell.userData = { baseY: 0.42, role: 'trap_shell' };
        const coreSpike = new THREE.Mesh(new THREE.ConeGeometry(0.28, 0.7, 5), faultMat);
        coreSpike.rotation.x = Math.PI;
        coreSpike.position.set(0, 0.42, 0);
        coreSpike.userData = { baseY: 0.42, role: 'trap_spike' };
        this.hologramParts.push(shell, coreSpike);
        this.hologramGroup.add(shell, coreSpike);
        break;
      }
      case 'AXIOM': {
        const crystal = new THREE.Mesh(new THREE.OctahedronGeometry(0.45, 0), holoMat());
        crystal.position.set(0, 0.52, 0);
        crystal.userData = { baseY: 0.52, role: 'axiom_crystal' };
        const orbit = new THREE.Mesh(new THREE.TorusGeometry(0.68, 0.035, 6, 20), holoMat());
        orbit.position.set(0, 0.52, 0);
        orbit.userData = { baseY: 0.52, role: 'axiom_orbit' };
        this.hologramParts.push(crystal, orbit);
        this.hologramGroup.add(crystal, orbit);
        break;
      }
    }

    this.recomputeSceneTriangles();
  }

  private recomputeSceneTriangles(): void {
    this.totalSceneTriangles =
      groupTriangles(this.hudGridGroup) +
      groupTriangles(this.fortressGroup) +
      groupTriangles(this.hologramGroup);
  }

  private applyHologramDeformation(nowMs: number): void {
    const gain = this.state.parameterValues.gain ?? 4;
    const damping = this.state.parameterValues.damping ?? 2;
    const reserve = this.state.parameterValues.reserve ?? 6;
    const explode = this.state.track?.explode_factor ?? 0.1;
    const palette = THEME_PALETTES[this.state.theme];
    const pulse = this.state.reducedMotion ? 1 : 0.92 + 0.08 * Math.sin(nowMs * 0.004);

    for (const part of this.hologramParts) {
      const role = part.userData?.role as string | undefined;
      if (role === 'pipeline_stage') {
        const idx = Number(part.userData.idx ?? 0);
        part.position.x = Number(part.userData.baseX ?? 0) * (1 + explode * 0.55);
        part.scale.setScalar((0.75 + gain * 0.06) * pulse * (1 - idx * 0.05));
      } else if (role === 'pipeline_flux') {
        part.scale.set(1 + explode * 0.45, 0.6 + gain * 0.14, 0.6 + damping * 0.15);
      } else if (role === 'tradeoff_beam') {
        part.rotation.z = THREE.MathUtils.clamp((gain - damping - 2) * 0.08, -0.45, 0.45);
      } else if (role === 'tradeoff_left') {
        part.position.y = Number(part.userData.baseY ?? 0.5) - (gain - 4) * 0.04;
      } else if (role === 'tradeoff_right') {
        part.position.y = Number(part.userData.baseY ?? 0.5) + (reserve - 6) * 0.03 + explode * 0.2;
      } else if (role === 'spatial_layer') {
        const layer = Number(part.userData.layer ?? 0);
        part.position.y = Number(part.userData.baseY ?? 0) + layer * explode * 0.48;
      } else if (role === 'contrast_a') {
        part.position.x = Number(part.userData.baseX ?? 0) - explode * 0.35;
        part.scale.y = Math.max(0.35, gain / 5);
      } else if (role === 'contrast_b') {
        part.position.x = Number(part.userData.baseX ?? 0) + explode * 0.35;
        part.scale.y = Math.max(0.35, reserve / 7);
      } else if (role === 'trap_shell') {
        part.position.y = Number(part.userData.baseY ?? 0.42) + explode * 0.55;
      } else if (role === 'trap_spike') {
        part.scale.setScalar(0.85 + Math.max(0, gain - damping * 2) * 0.07);
      } else if (role === 'axiom_crystal') {
        part.scale.setScalar((0.8 + (gain + damping) * 0.045) * (this.state.evaluationAllPassed ? 1.08 : 0.95));
        if (!this.state.reducedMotion) part.rotation.y = nowMs * 0.0012;
        const mat = (part as THREE.Mesh).material as THREE.MeshStandardMaterial;
        mat.color.setHex(this.state.evaluationAllPassed ? palette.healthyHex : palette.accentHex);
      } else if (role === 'axiom_orbit' && !this.state.reducedMotion) {
        part.rotation.x = Math.PI / 3 + explode * 0.5;
        part.rotation.y = -nowMs * 0.0016;
      }
    }
  }

  private updateCamera(deltaSec: number): void {
    const selected = this.state.fortress.nodes.find(n => n.node_id === this.state.selectedNodeId)
      ?? this.state.fortress.nodes[0];
    const basePos = selected ? this.createNodePosition(selected) : new THREE.Vector3();

    let desiredTarget: THREE.Vector3;
    let desiredPos: THREE.Vector3;

    if (this.state.cameraFocusMode === 'fortress') {
      desiredTarget = new THREE.Vector3(0.2, 0.5, 0.2);
      desiredPos = new THREE.Vector3(6.8, 6.2, 7.8);
    } else {
      const roleOffset = this.state.track
        ? FOCUS_ANCHOR_POS[this.state.track.focus_role]
        : FOCUS_ANCHOR_POS.concept;
      desiredTarget = basePos.clone().add(roleOffset);
      const shot = this.state.track?.shot_type ?? 'overview';
      if (shot === 'close_up') {
        desiredPos = desiredTarget.clone().add(new THREE.Vector3(2.2, 1.45, 2.75));
      } else if (shot === 'compare') {
        desiredPos = desiredTarget.clone().add(new THREE.Vector3(0.4, 2.25, 4.1));
      } else {
        desiredPos = desiredTarget.clone().add(new THREE.Vector3(3.6, 2.5, 4.3));
      }
    }

    const alpha = this.state.reducedMotion ? 1 : Math.min(1, deltaSec * 6.5);
    this.currentPosLerp(desiredPos, desiredTarget, alpha);
    this.camera.position.copy(this.currentCamPos);
    this.camera.lookAt(this.currentTarget);
  }

  private currentPosLerp(desiredPos: THREE.Vector3, desiredTarget: THREE.Vector3, alpha: number): void {
    this.currentCamPos.lerp(desiredPos, alpha);
    this.currentTarget.lerp(desiredTarget, alpha);
  }

  private startLoop(): void {
    this.lastTickMs = performance.now();
    const step = (now: number) => {
      if (this.paused) {
        this.animId = null;
        return;
      }
      const deltaMs = Math.max(1, now - this.lastTickMs);
      this.lastTickMs = now;

      this.applyHologramDeformation(now);
      this.updateCamera(deltaMs / 1000);

      if (this.renderer) {
        this.renderer.render(this.scene, this.camera);
      }

      const drawCalls = this.renderer?.info.render.calls ?? 1;
      const reportedTriangles = this.renderer?.info.render.triangles ?? this.totalSceneTriangles;
      const frame = this.monitor.recordFrame(now, deltaMs, {
        drawCalls,
        triangles: reportedTriangles > 0 ? reportedTriangles : this.totalSceneTriangles,
      });

      const budget = validateAssetBudget({
        kind: 'scene',
        triangles: this.totalSceneTriangles,
        maxTextureWidth: 512,
        maxTextureHeight: 512,
        bufferBytes: this.totalSceneTriangles * 96,
        uris: [],
        hasExecutableScript: false,
      });

      this.onMetrics({
        frame,
        budget,
        unitTriangleMax: this.maxUnitTriangles,
        rendererBackend: this.rendererBackend,
        cameraTarget: [
          Number(this.currentTarget.x.toFixed(2)),
          Number(this.currentTarget.y.toFixed(2)),
          Number(this.currentTarget.z.toFixed(2)),
        ],
        activeExplodeFactor: this.state.track?.explode_factor ?? 0.1,
      });

      this.animId = requestAnimationFrame(step);
    };
    this.animId = requestAnimationFrame(step);
  }

  destroy(): void {
    if (this.animId !== null) {
      cancelAnimationFrame(this.animId);
      this.animId = null;
    }
    disposeGroup(this.hudGridGroup);
    disposeGroup(this.fortressGroup);
    disposeGroup(this.hologramGroup);
    if (this.renderer) {
      this.renderer.domElement.removeEventListener('pointerdown', this.handlePointerDown);
      this.renderer.dispose();
    }
  }
}
