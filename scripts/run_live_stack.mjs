#!/usr/bin/env node
// Starts or reuses the private PostgreSQL 16 cluster in artifacts/private/live-dev,
// runs migrations + M1..M7 grants, seeds playable demo data (idempotently), and
// launches campuslingo-server serving packages/game-client/public + /wasm/*.

import { spawn, spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pgBin =
  process.env.CAMPUSLINGO_PG_BIN ??
  '/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin';
const liveWorkDir = path.join(root, 'artifacts', 'private', 'live-dev');
const dataDir = path.join(liveWorkDir, 'data');
const socketDir = path.join(liveWorkDir, 'socket');
const pgLogPath = path.join(liveWorkDir, 'postgres.log');
const assetDir = path.join(liveWorkDir, 'assets');
const credsPath = path.join(liveWorkDir, 'credentials.json');

export const DEMO_PASSWORD = 'CampusLingo#2026!';
export const STUDENT_EMAIL = 'student@campuslingo.local';
export const DEFENDER_EMAIL = 'defender@campuslingo.local';
export const DEMO_TENANT_ID = '00000000-0000-4000-8000-000000000001';
export const DEMO_COURSE_ID = '00000000-0000-4000-8000-000000000201';
export const DEMO_CONTENT_VERSION_ID = '00000000-0000-4000-8000-000000000301';
export const DEMO_BOSS_ID = '00000000-0000-4000-8000-000000000501';
export const DEMO_CLAN_ID = '00000000-0000-4000-8000-000000000601';

const DEMO_NODES = [
  {
    id: '00000000-0000-4000-8000-000000000401',
    title: 'Linear Operators',
    archetype: 'PIPELINE',
    modifier: 'FAULT_INJECTION',
    grid_x: 0,
    grid_y: 0,
    cost: 3,
  },
  {
    id: '00000000-0000-4000-8000-000000000402',
    title: 'Eigenvalue Bastion',
    archetype: 'AXIOM',
    modifier: 'COROLLARY_CHAIN',
    grid_x: 2,
    grid_y: 0,
    cost: 2,
  },
  {
    id: '00000000-0000-4000-8000-000000000403',
    title: 'Laplace Gate',
    archetype: 'SPATIAL',
    modifier: 'CROSS_SECTION_SCAN',
    grid_x: 1,
    grid_y: 2,
    cost: 3,
  },
  {
    id: '00000000-0000-4000-8000-000000000404',
    title: 'Stability Core',
    archetype: 'TRAP',
    modifier: 'SAFEGUARD_PATCH',
    grid_x: 3,
    grid_y: 2,
    cost: 4,
  },
];

function currentIsoWeekInfo() {
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
  const monday = new Date(date);
  monday.setUTCDate(date.getUTCDate() - 3);
  return {
    weekKey: `${date.getUTCFullYear()}-W${String(weekNo).padStart(2, '0')}`,
    weekStartDate: monday.toISOString().slice(0, 10),
  };
}

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

export async function freePort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const chosen = typeof address === 'object' && address ? address.port : 0;
      server.close(() => resolve(chosen));
    });
  });
}

function runSync(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    encoding: 'utf8',
    env: { ...process.env, ...options.env },
  });
  if (result.error) throw result.error;
  if (result.status !== 0 && !options.allowFailure) {
    const detail = options.capture ? `\n${result.stderr}\n${result.stdout}` : '';
    throw new Error(`${command} ${args.join(' ')} exited with ${result.status}${detail}`);
  }
  return result;
}

function psqlSync(args, pgHost, pgPort, capture = false) {
  return runSync(path.join(pgBin, 'psql'), args, {
    capture,
    env: { PGHOST: pgHost, PGPORT: String(pgPort) },
  });
}

function pgUrl(role, password, pgPort, databaseName = 'campuslingo_live') {
  return `postgres://${role}:${password}@127.0.0.1:${pgPort}/${databaseName}`;
}

async function loadOrCreateCredentials() {
  if (await exists(credsPath)) {
    const raw = await readFile(credsPath, 'utf8');
    return JSON.parse(raw);
  }
  return {
    passwords: {
      owner: randomBytes(24).toString('hex'),
      auth: randomBytes(24).toString('hex'),
      app: randomBytes(24).toString('hex'),
      grader: randomBytes(24).toString('hex'),
      worker: randomBytes(24).toString('hex'),
      recovery: randomBytes(24).toString('hex'),
    },
    recoveryKey: randomBytes(32).toString('hex'),
  };
}

export async function ensureLiveDatabase(options = {}) {
  const databaseName = options.databaseName ?? 'campuslingo_live';
  if (!/^[a-z0-9_]+$/i.test(databaseName)) {
    throw new Error(`Invalid databaseName: ${databaseName}`);
  }
  await mkdir(liveWorkDir, { recursive: true, mode: 0o700 });
  await mkdir(socketDir, { recursive: true, mode: 0o700 });
  await mkdir(assetDir, { recursive: true, mode: 0o700 });

  const creds = await loadOrCreateCredentials();
  const hasCluster = await exists(path.join(dataDir, 'PG_VERSION'));
  if (!hasCluster) {
    runSync(path.join(pgBin, 'initdb'), [
      '-D',
      dataDir,
      '-U',
      'postgres',
      '--auth-local=trust',
      '--auth-host=scram-sha-256',
      '--encoding=UTF8',
      '--locale=C',
      '--no-sync',
    ]);
  }

  let statusRes = runSync(path.join(pgBin, 'pg_ctl'), ['-D', dataDir, 'status'], {
    capture: true,
    allowFailure: true,
  });

  let pgPort = 0;
  let pgHost = socketDir;

  if (statusRes.status === 0 && (await exists(path.join(dataDir, 'postmaster.pid')))) {
    const pidLines = (await readFile(path.join(dataDir, 'postmaster.pid'), 'utf8'))
      .split('\n')
      .map(s => s.trim());
    pgPort = Number.parseInt(pidLines[3] ?? '0', 10);
    pgHost = pidLines[4] || socketDir;
  } else {
    if (await exists(path.join(dataDir, 'postmaster.pid'))) {
      await rm(path.join(dataDir, 'postmaster.pid'), { force: true });
    }
    pgPort = await freePort();
    pgHost = socketDir;
    runSync(path.join(pgBin, 'pg_ctl'), [
      '-D',
      dataDir,
      '-l',
      pgLogPath,
      '-o',
      `-p ${pgPort} -k ${socketDir} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700 -c max_connections=100`,
      '-w',
      'start',
    ]);
  }

  const { passwords, recoveryKey } = creds;
  const roleSyncSql = `
SET password_encryption = 'scram-sha-256';
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_owner') THEN
    CREATE ROLE campuslingo_owner LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
  ELSE
    ALTER ROLE campuslingo_owner WITH LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_auth') THEN
    CREATE ROLE campuslingo_auth LOGIN PASSWORD '${passwords.auth}';
  ELSE
    ALTER ROLE campuslingo_auth WITH LOGIN PASSWORD '${passwords.auth}';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_app') THEN
    CREATE ROLE campuslingo_app LOGIN PASSWORD '${passwords.app}';
  ELSE
    ALTER ROLE campuslingo_app WITH LOGIN PASSWORD '${passwords.app}';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_grader') THEN
    CREATE ROLE campuslingo_grader LOGIN PASSWORD '${passwords.grader}';
  ELSE
    ALTER ROLE campuslingo_grader WITH LOGIN PASSWORD '${passwords.grader}';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_worker') THEN
    CREATE ROLE campuslingo_worker LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
  ELSE
    ALTER ROLE campuslingo_worker WITH LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'campuslingo_recovery') THEN
    CREATE ROLE campuslingo_recovery LOGIN PASSWORD '${passwords.recovery}';
  ELSE
    ALTER ROLE campuslingo_recovery WITH LOGIN PASSWORD '${passwords.recovery}';
  END IF;
END $$;
`;
  psqlSync(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', roleSyncSql], pgHost, pgPort, true);

  const dbCheck = psqlSync(
    ['-tAc', `SELECT 1 FROM pg_database WHERE datname = '${databaseName}'`, '-U', 'postgres', '-d', 'postgres'],
    pgHost,
    pgPort,
    true,
  );
  if (dbCheck.stdout.trim() !== '1') {
    psqlSync(
      ['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', `CREATE DATABASE ${databaseName} OWNER campuslingo_owner`],
      pgHost,
      pgPort,
      true,
    );
  }
  psqlSync(
    ['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', databaseName, '-c', 'CREATE EXTENSION IF NOT EXISTS vector'],
    pgHost,
    pgPort,
    true,
  );

  await writeFile(credsPath, JSON.stringify({ passwords, recoveryKey }, null, 2) + '\n', { mode: 0o600 });

  const ownerUrl = pgUrl('campuslingo_owner', passwords.owner, pgPort, databaseName);
  const releaseBin = path.join(root, 'target', 'release', 'campuslingo-server');
  if (await exists(releaseBin)) {
    runSync(releaseBin, ['migrate'], {
      env: { CAMPUSLINGO_ADMIN_DATABASE_URL: ownerUrl },
    });
  } else {
    runSync(process.env.CARGO ?? 'cargo', ['run', '--quiet', '-p', 'campuslingo-server', '--', 'migrate'], {
      env: { CAMPUSLINGO_ADMIN_DATABASE_URL: ownerUrl },
    });
  }

  for (const grantFile of [
    'm1_grants.sql',
    'm2_grants.sql',
    'm3_grants.sql',
    'm4_grants.sql',
    'm5_grants.sql',
    'm6_grants.sql',
    'm7_grants.sql',
  ]) {
    psqlSync(
      ['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', databaseName, '-f', path.join(root, 'scripts', grantFile)],
      pgHost,
      pgPort,
      true,
    );
  }

  return {
    pgHost,
    pgPort,
    pgBin,
    databaseName,
    env: {
      CAMPUSLINGO_AUTH_DATABASE_URL: pgUrl('campuslingo_auth', passwords.auth, pgPort, databaseName),
      CAMPUSLINGO_APP_DATABASE_URL: pgUrl('campuslingo_app', passwords.app, pgPort, databaseName),
      CAMPUSLINGO_GRADER_DATABASE_URL: pgUrl('campuslingo_grader', passwords.grader, pgPort, databaseName),
      CAMPUSLINGO_ADMIN_DATABASE_URL: ownerUrl,
      CAMPUSLINGO_RECOVERY_DATABASE_URL: pgUrl('campuslingo_recovery', passwords.recovery, pgPort, databaseName),
      CAMPUSLINGO_RECOVERY_KEY: recoveryKey,
      CAMPUSLINGO_ASSET_DIR: assetDir,
      CAMPUSLINGO_WEB_DIR: path.join(root, 'packages', 'game-client', 'public'),
      CAMPUSLINGO_WASM_DIR: path.join(root, 'packages', 'engine', 'generated'),
    },
  };
}

async function ensureDemoUser(baseUrl, email, password, dbInfo) {
  let res = await fetch(`${baseUrl}/v1/auth/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (res.status === 201 || res.status === 200) {
    const json = await res.json();
    return json.user_id;
  }

  res = await fetch(`${baseUrl}/v1/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (res.ok) {
    const json = await res.json();
    return json.user_id;
  }

  // Existing demo row may hold a legacy/unusable hash (e.g. 'demo-unusable').
  // Hash password via a temporary registration, copy its PHC hash, then log in.
  const tmpEmail = `seed-tmp-${Date.now()}-${randomBytes(4).toString('hex')}@campuslingo.local`;
  const tmpRes = await fetch(`${baseUrl}/v1/auth/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: tmpEmail, password }),
  });
  if (!tmpRes.ok) {
    throw new Error(`Failed to provision hash for ${email}: HTTP ${tmpRes.status}`);
  }
  const tmpJson = await tmpRes.json();
  const safeEmail = email.replace(/'/g, "''");
  const syncSql = `
BEGIN;
UPDATE users
SET password_hash = (SELECT password_hash FROM users WHERE id = '${tmpJson.user_id}'::uuid)
WHERE email = '${safeEmail}';
DELETE FROM users WHERE id = '${tmpJson.user_id}'::uuid;
COMMIT;
`;
  psqlSync(
    ['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', dbInfo.databaseName ?? 'campuslingo_live', '-c', syncSql],
    dbInfo.pgHost,
    dbInfo.pgPort,
    true,
  );

  res = await fetch(`${baseUrl}/v1/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) {
    throw new Error(`Failed to register or login ${email}: HTTP ${res.status}`);
  }
  const json = await res.json();
  return json.user_id;
}

export async function seedDemoData(baseUrl, dbInfo) {
  const studentId = await ensureDemoUser(baseUrl, STUDENT_EMAIL, DEMO_PASSWORD, dbInfo);
  const defenderId = await ensureDemoUser(baseUrl, DEFENDER_EMAIL, DEMO_PASSWORD, dbInfo);
  const { weekKey, weekStartDate } = currentIsoWeekInfo();
  const sha = 'a'.repeat(64);
  const leagueScope = `COURSE:${DEMO_COURSE_ID}`;

  const nodeValues = DEMO_NODES.map(
    n => `('${n.id}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${n.title}')`,
  ).join(',\n  ');

  const bundleValues = DEMO_NODES.map(
    n =>
      `('${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${n.id}'::uuid, '${DEMO_CONTENT_VERSION_ID}'::uuid, '${n.archetype}', '${n.modifier}', ${n.cost}, 'BIO_SYSTEM', 'en', ${n.grid_x}, ${n.grid_y}, '{"seed":true}'::jsonb, '${sha}', 'seed-generator', 'r1', '1.0.0', '1.0.0')`,
  ).join(',\n  ');

  const challenges = [
    ['00000000-0000-4000-8000-000000000701', DEMO_NODES[0].id, 'Linear Operator Pipeline: Which step preserves invariant rank across basis change?', '["Apply invertible similarity P^-1 A P","Drop pivot column arbitrarily","Zero out non-diagonal trace entries","Invert singular kernel matrix"]'],
    ['00000000-0000-4000-8000-000000000702', DEMO_NODES[1].id, 'Eigenvalue Axiom: If Av = 3v cleanly scales eigenvector v, what is det(A - 3I)?', '["0 because (A - 3I) has nontrivial kernel","3 because trace shifts by 3","1 because v is normalized","Undefined unless A is symmetric"]'],
    ['00000000-0000-4000-8000-000000000703', DEMO_NODES[2].id, 'Laplace Spatial Gate: Which pole region guarantees asymptotic stability for continuous LTI systems?', '["Strict left-half s-plane Re(s) < 0","Right-half s-plane Re(s) > 0","Imaginary axis Re(s) = 0 with double pole","Outer circle |s| > 1"]'],
    ['00000000-0000-4000-8000-000000000704', DEMO_NODES[3].id, 'Safeguard Patch: What stops division-by-zero blowup when damping coefficient approaches zero?', '["Clamp denominator >= epsilon threshold","Remove feedback integral branch","Switch all gains to negative sign","Double integration step size"]'],
    ['00000000-0000-4000-8000-000000000705', DEMO_NODES[0].id, 'Corollary Chain: What equals trace(A) for any n x n complex matrix A?', '["Sum of eigenvalues counted with multiplicity","Product of eigenvalues with sign","Maximum singular value squared","Condition number divided by norm"]'],
    ['00000000-0000-4000-8000-000000000706', DEMO_NODES[1].id, 'Cross-Section Scan: Orthogonal eigenvectors of a real symmetric matrix satisfy which dot product?', '["v_i . v_j = 0 for distinct eigenvalues","v_i . v_j = 1 for all pairs","v_i . v_j = lambda_i + lambda_j","v_i . v_j = det(A)"]'],
  ];

  const challengeRowsSql = challenges
    .map(
      ([cid, nid, prompt, options]) =>
        `('${cid}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${nid}'::uuid, '${prompt.replace(/'/g, "''")}', '${options.replace(/'/g, "''")}'::jsonb, 1)`,
    )
    .join(',\n  ');

  const answerRowsSql = challenges
    .map(([cid]) => `('${cid}'::uuid, 0, 'Seeded canonical option 0 is verified correct.')`)
    .join(',\n  ');

  const shieldRowsSql = [studentId, defenderId]
    .flatMap(uid =>
      DEMO_NODES.map(
        n =>
          `('${uid}'::uuid, '${n.id}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${DEMO_CONTENT_VERSION_ID}'::uuid, 'ACTIVE', 2, 6800, 7200, 140, FALSE, FALSE, FALSE, '{}'::text[], now())`,
      ),
    )
    .join(',\n  ');

  const sql = `
BEGIN;

INSERT INTO tenants (id, owner_id, name)
VALUES ('${DEMO_TENANT_ID}'::uuid, '${defenderId}'::uuid, 'CampusLingo Live Academy')
ON CONFLICT (id) DO NOTHING;

INSERT INTO tenant_members (tenant_id, user_id, role, active)
VALUES
  ('${DEMO_TENANT_ID}'::uuid, '${defenderId}'::uuid, 'owner', TRUE),
  ('${DEMO_TENANT_ID}'::uuid, '${studentId}'::uuid, 'student', TRUE)
ON CONFLICT (tenant_id, user_id) DO UPDATE SET active = TRUE;

INSERT INTO courses (id, tenant_id, created_by, name, visibility, published, rights_attested_at, published_by, raid_opt_in_default)
VALUES ('${DEMO_COURSE_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${defenderId}'::uuid, 'Linear Systems & Hologram Citadel', 'public', TRUE, now(), '${defenderId}'::uuid, TRUE)
ON CONFLICT (id) DO UPDATE SET published = TRUE, visibility = 'public', raid_opt_in_default = TRUE;

INSERT INTO course_members (course_id, tenant_id, user_id, role, active, raid_opt_in)
VALUES
  ('${DEMO_COURSE_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${defenderId}'::uuid, 'instructor', TRUE, TRUE),
  ('${DEMO_COURSE_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${studentId}'::uuid, 'student', TRUE, TRUE)
ON CONFLICT (course_id, user_id) DO UPDATE SET active = TRUE, raid_opt_in = TRUE;

INSERT INTO learning_nodes (id, tenant_id, course_id, title)
VALUES
  ${nodeValues}
ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title;

INSERT INTO node_prerequisites (prerequisite_node_id, dependent_node_id, tenant_id, course_id, weight)
VALUES
  ('${DEMO_NODES[0].id}'::uuid, '${DEMO_NODES[1].id}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, 0.8),
  ('${DEMO_NODES[0].id}'::uuid, '${DEMO_NODES[2].id}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, 0.7),
  ('${DEMO_NODES[1].id}'::uuid, '${DEMO_NODES[3].id}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, 0.9)
ON CONFLICT (prerequisite_node_id, dependent_node_id) DO NOTHING;

INSERT INTO content_versions (id, tenant_id, course_id, version_number, status)
VALUES ('${DEMO_CONTENT_VERSION_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, 1, 'published')
ON CONFLICT (id) DO UPDATE SET status = 'published';

INSERT INTO learning_unit_bundles (
  tenant_id, course_id, node_id, content_version_id, archetype, modifier, card_elixir_cost,
  world_theme, language, grid_x, grid_y, bundle_json, source_sha256,
  generator_model_id, generator_revision, generator_version, schema_version
)
VALUES
  ${bundleValues}
ON CONFLICT (node_id, content_version_id) DO NOTHING;

INSERT INTO challenges (id, tenant_id, course_id, node_id, prompt, options, version)
VALUES
  ${challengeRowsSql}
ON CONFLICT (id) DO NOTHING;

INSERT INTO challenge_answers (challenge_id, correct_index, explanation)
VALUES
  ${answerRowsSql}
ON CONFLICT (challenge_id) DO NOTHING;

INSERT INTO user_protection_state (
  user_id, tenant_id, last_qualifying_study_at, is_dormant,
  new_player_bubble_until, eligible_transition_count, consecutive_dormant_weeks,
  accommodation_multiplier, accommodation_version
)
VALUES
  ('${studentId}'::uuid, '${DEMO_TENANT_ID}'::uuid, now(), FALSE, now() - INTERVAL '2 hours', 30, 0, 1.0, 1),
  ('${defenderId}'::uuid, '${DEMO_TENANT_ID}'::uuid, now(), FALSE, now() - INTERVAL '2 hours', 30, 0, 1.0, 1)
ON CONFLICT (user_id) DO UPDATE
SET new_player_bubble_until = now() - INTERVAL '2 hours',
    eligible_transition_count = GREATEST(user_protection_state.eligible_transition_count, 30),
    is_dormant = FALSE;

INSERT INTO user_node_shield_state (
  user_id, node_id, tenant_id, course_id, content_version_id,
  state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout,
  raid_immune, propagation_exempt, active_protections, computed_at
)
VALUES
  ${shieldRowsSql}
ON CONFLICT (user_id, node_id) DO NOTHING;

INSERT INTO chapter_bosses (
  id, tenant_id, course_id, node_id, title, mode, max_hp, current_hp,
  reward_tier, gold_reward, cosmetic_reward_key, status, expires_at
)
VALUES (
  '${DEMO_BOSS_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${DEMO_NODES[3].id}'::uuid,
  'Eigen-Warden Construct', 'SOLO', 300, 300,
  'BOSS_CHEST_APEX', 150, 'apex_warden_halo', 'ACTIVE', now() + INTERVAL '7 days'
)
ON CONFLICT (id) DO NOTHING;

WITH bracket_ins AS (
  INSERT INTO league_brackets (league_scope, course_id, week_key, week_start_date, tier)
  SELECT '${leagueScope}', '${DEMO_COURSE_ID}'::uuid, '${weekKey}', '${weekStartDate}'::date, 1
  WHERE NOT EXISTS (
    SELECT 1 FROM league_brackets
    WHERE league_scope = '${leagueScope}' AND course_id = '${DEMO_COURSE_ID}'::uuid AND week_key = '${weekKey}' AND tier = 1
  )
  RETURNING id
),
resolved_bracket AS (
  SELECT id FROM bracket_ins
  UNION ALL
  SELECT id FROM league_brackets
  WHERE league_scope = '${leagueScope}' AND course_id = '${DEMO_COURSE_ID}'::uuid AND week_key = '${weekKey}' AND tier = 1
  LIMIT 1
)
INSERT INTO league_standings (
  bracket_id, league_scope, course_id, week_key, user_id, timezone,
  tier_before, trophies_earned, eligible_mastery_recovery_count,
  is_dormant, returned_from_dormancy_this_week, qualifying_sessions_since_return, consecutive_dormant_weeks
)
SELECT rb.id, '${leagueScope}', '${DEMO_COURSE_ID}'::uuid, '${weekKey}', v.uid, 'Asia/Kuala_Lumpur',
       1, v.trophies, v.recovery, FALSE, FALSE, 2, 0
FROM resolved_bracket rb
CROSS JOIN (
  VALUES
    ('${studentId}'::uuid, 120, 4),
    ('${defenderId}'::uuid, 95, 3)
) AS v(uid, trophies, recovery)
ON CONFLICT (league_scope, week_key, user_id) DO NOTHING;

INSERT INTO clans (id, tenant_id, course_id, name, created_by)
VALUES ('${DEMO_CLAN_ID}'::uuid, '${DEMO_TENANT_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, 'Eigen Vanguard', '${defenderId}'::uuid)
ON CONFLICT (id) DO NOTHING;

INSERT INTO clan_memberships (clan_id, course_id, user_id, role)
VALUES
  ('${DEMO_CLAN_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${defenderId}'::uuid, 'LEADER'),
  ('${DEMO_CLAN_ID}'::uuid, '${DEMO_COURSE_ID}'::uuid, '${studentId}'::uuid, 'MEMBER')
ON CONFLICT (course_id, user_id) DO NOTHING;

COMMIT;
`;

  psqlSync(
    ['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', dbInfo.databaseName ?? 'campuslingo_live', '-c', sql],
    dbInfo.pgHost,
    dbInfo.pgPort,
    true,
  );

  return { studentId, defenderId, courseId: DEMO_COURSE_ID, weekKey };
}

export async function waitForServerReady(baseUrl, child, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child && child.exitCode !== null) {
      throw new Error(`campuslingo-server exited early with code ${child.exitCode}`);
    }
    try {
      const res = await fetch(`${baseUrl}/readyz`);
      if (res.ok) return;
    } catch {
      // Retry until ready
    }
    await new Promise(r => setTimeout(r, 250));
  }
  throw new Error(`Timed out waiting for ${baseUrl}/readyz`);
}

async function isBindAvailable(bind) {
  const lastColon = bind.lastIndexOf(':');
  const host = lastColon > 0 ? bind.slice(0, lastColon) : '127.0.0.1';
  const port = Number.parseInt(bind.slice(lastColon + 1), 10);
  if (!Number.isInteger(port) || port <= 0) {
    return false;
  }
  return await new Promise(resolve => {
    const server = net.createServer();
    server.once('error', () => resolve(false));
    server.listen(port, host, () => server.close(() => resolve(true)));
  });
}

export async function startServerProcess(dbInfo, options = {}) {
  const explicitBind = options.bind ?? process.env.CAMPUSLINGO_BIND;
  let bind = explicitBind ?? '127.0.0.1:18080';
  if (!(await isBindAvailable(bind))) {
    if (explicitBind) {
      throw new Error(
        `Bind address ${bind} is already in use by another process. Free that port or choose another via CAMPUSLINGO_BIND / --port.`,
      );
    }
    const fallbackPort = await freePort();
    console.warn(
      `Warning: default 127.0.0.1:18080 is in use; binding campuslingo-server to 127.0.0.1:${fallbackPort} instead.`,
    );
    bind = `127.0.0.1:${fallbackPort}`;
  }
  const baseUrl = `http://${bind}`;
  const releaseBin = path.join(root, 'target', 'release', 'campuslingo-server');
  const useRelease = await exists(releaseBin);

  const cmd = useRelease ? releaseBin : (process.env.CARGO ?? 'cargo');
  const args = useRelease ? [] : ['run', '--quiet', '-p', 'campuslingo-server'];

  const child = spawn(cmd, args, {
    cwd: root,
    stdio: options.quiet ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    detached: Boolean(options.daemon),
    env: {
      ...process.env,
      ...dbInfo.env,
      CAMPUSLINGO_BIND: bind,
    },
  });

  if (options.daemon) {
    child.unref();
  }

  await waitForServerReady(baseUrl, child, useRelease ? 30_000 : 120_000);
  const seeded = await seedDemoData(baseUrl, dbInfo);
  return { child, baseUrl, bind, seeded };
}

async function main() {
  const argv = process.argv.slice(2);
  const args = new Set(argv);
  const daemon = args.has('--daemon');
  const seedOnly = args.has('--seed-only');
  const portFlagIdx = argv.indexOf('--port');
  const cliPort =
    portFlagIdx >= 0 && argv[portFlagIdx + 1] ? argv[portFlagIdx + 1] : null;
  const requestedBind =
    process.env.CAMPUSLINGO_BIND ??
    (cliPort ? `127.0.0.1:${cliPort}` : seedOnly ? `127.0.0.1:${await freePort()}` : undefined);

  const dbInfo = await ensureLiveDatabase();
  const { child, baseUrl, seeded } = await startServerProcess(dbInfo, {
    ...(requestedBind ? { bind: requestedBind } : {}),
    daemon,
    quiet: seedOnly,
  });

  console.log(
    JSON.stringify({
      status: 'ready',
      url: baseUrl,
      demoAccount: { email: STUDENT_EMAIL, password: DEMO_PASSWORD },
      seeded,
      pgPort: dbInfo.pgPort,
    }),
  );

  if (seedOnly) {
    child.kill('SIGTERM');
    return;
  }
  if (daemon) {
    return;
  }

  const stop = () => {
    child.kill('SIGTERM');
    process.exit(0);
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  child.on('exit', code => process.exit(code ?? 0));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
