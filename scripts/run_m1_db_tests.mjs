#!/usr/bin/env node
// M1 integration harness. A private PostgreSQL 16 cluster is initialized from
// the read-only toolchain used by the earlier project; no Docker or system
// service is required. Certificates, passwords, and databases live under
// artifacts/private and are removed unless CAMPUSLINGO_KEEP_PG=1 is set.

import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdir, rm, stat, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pgBin =
  process.env.CAMPUSLINGO_PG_BIN ??
  '/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin';
const work = path.join(root, 'artifacts', 'private', 'pgtest', 'm1');
const data = path.join(work, 'data');
const socket = path.join(work, 'socket');
const log = path.join(work, 'postgres.log');
const assetDir = path.join(work, 'assets');
const keep = process.env.CAMPUSLINGO_KEEP_PG === '1';

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, ...options.env },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited with ${result.status}`);
  }
}

function psql(args) {
  return spawnSync(path.join(pgBin, 'psql'), args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, PGHOST: socket, PGPORT: String(port) },
  });
}

function requireSuccess(result, label) {
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${label} failed with ${result.status}`);
}

async function freePort() {
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

const port = await freePort();
const passwords = {
  owner: randomBytes(24).toString('hex'),
  auth: randomBytes(24).toString('hex'),
  app: randomBytes(24).toString('hex'),
  grader: randomBytes(24).toString('hex'),
  worker: randomBytes(24).toString('hex'),
  recovery: randomBytes(24).toString('hex'),
};

function url(role, password) {
  return `postgres://${role}:${password}@127.0.0.1:${port}/campuslingo_test`;
}

let started = false;
try {
  await stat(path.join(pgBin, 'initdb'));
  await rm(work, { recursive: true, force: true });
  await mkdir(socket, { recursive: true, mode: 0o700 });
  await mkdir(assetDir, { recursive: true, mode: 0o700 });

  run(path.join(pgBin, 'initdb'), [
    '-D',
    data,
    '-U',
    'postgres',
    '--auth-local=trust',
    '--auth-host=scram-sha-256',
    '--encoding=UTF8',
    '--locale=C',
    '--no-sync',
  ]);
  run(path.join(pgBin, 'pg_ctl'), [
    '-D',
    data,
    '-l',
    log,
    '-o',
    `-p ${port} -k ${socket} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700 -c max_connections=100`,
    '-w',
    'start',
  ]);
  started = true;

  const roles = `
SET password_encryption = 'scram-sha-256';
CREATE ROLE campuslingo_owner LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
CREATE ROLE campuslingo_auth LOGIN PASSWORD '${passwords.auth}';
CREATE ROLE campuslingo_app LOGIN PASSWORD '${passwords.app}';
CREATE ROLE campuslingo_grader LOGIN PASSWORD '${passwords.grader}';
CREATE ROLE campuslingo_worker LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
CREATE ROLE campuslingo_recovery LOGIN PASSWORD '${passwords.recovery}';
`;
  requireSuccess(
    psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', roles]),
    'role bootstrap',
  );
  requireSuccess(
    psql([
      '-v',
      'ON_ERROR_STOP=1',
      '-U',
      'postgres',
      '-d',
      'postgres',
      '-c',
      'CREATE DATABASE campuslingo_test OWNER campuslingo_owner',
    ]),
    'database bootstrap',
  );

  run(process.env.CARGO ?? 'cargo', ['run', '--quiet', '-p', 'campuslingo-server', '--', 'migrate'], {
    env: {
      CAMPUSLINGO_ADMIN_DATABASE_URL: url('campuslingo_owner', passwords.owner),
    },
  });

  requireSuccess(
    psql([
      '-v',
      'ON_ERROR_STOP=1',
      '-U',
      'campuslingo_owner',
      '-d',
      'campuslingo_test',
      '-f',
      path.join(root, 'scripts', 'm1_grants.sql'),
    ]),
    'grant application',
  );

  if (keep) {
    await writeFile(
      path.join(work, 'connection.json'),
      JSON.stringify(
        {
          host: '127.0.0.1',
          port,
          postgres: { user: 'postgres', database: 'postgres' },
          admin: url('campuslingo_owner', passwords.owner),
          auth: url('campuslingo_auth', passwords.auth),
          app: url('campuslingo_app', passwords.app),
          grader: url('campuslingo_grader', passwords.grader),
          worker: url('campuslingo_worker', passwords.worker),
          recovery: url('campuslingo_recovery', passwords.recovery),
        },
        null,
        2,
      ) + '\n',
      { mode: 0o600 },
    );
  }

  const testArgs = [
    'test',
    '-p',
    'campuslingo-server',
    '--test',
    'm1',
    '--',
    '--ignored',
    '--test-threads=1',
    '--nocapture',
  ];
  if (process.env.CAMPUSLINGO_TEST_FILTER) {
    testArgs.push(process.env.CAMPUSLINGO_TEST_FILTER);
  }
  run(process.env.CARGO ?? 'cargo', testArgs, {
    env: {
      CAMPUSLINGO_TEST_ADMIN_URL: url('campuslingo_owner', passwords.owner),
      CAMPUSLINGO_TEST_AUTH_URL: url('campuslingo_auth', passwords.auth),
      CAMPUSLINGO_TEST_APP_URL: url('campuslingo_app', passwords.app),
      CAMPUSLINGO_TEST_GRADER_URL: url('campuslingo_grader', passwords.grader),
      CAMPUSLINGO_TEST_WORKER_URL: url('campuslingo_worker', passwords.worker),
      CAMPUSLINGO_TEST_RECOVERY_URL: url('campuslingo_recovery', passwords.recovery),
      CAMPUSLINGO_TEST_ASSET_DIR: assetDir,
      CAMPUSLINGO_RECOVERY_KEY: randomBytes(32).toString('hex'),
      RUST_BACKTRACE: '1',
    },
  });
} finally {
  if (started) {
    spawnSync(path.join(pgBin, 'pg_ctl'), ['-D', data, '-m', 'immediate', 'stop'], {
      cwd: root,
      stdio: 'inherit',
    });
  }
  if (!keep) {
    await rm(work, { recursive: true, force: true });
  } else {
    console.log(`kept private cluster at ${work}`);
  }
}
