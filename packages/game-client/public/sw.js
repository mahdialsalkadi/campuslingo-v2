const CACHE_NAME = 'campuslingo-static-v1';

const STATIC_ALLOWLIST = new Set([
  '/',
  '/index.html',
  '/styles.css',
  '/m4-shell.css',
  '/app.mjs',
  '/controller.js',
  '/shell.js',
  '/manifest.json',
  '/icons/icon-192.svg',
  '/icons/icon-512.svg',
]);

function isPermittedAssetRequest(request) {
  if (request.method !== 'GET') {
    return false;
  }
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) {
    return false;
  }
  if (url.pathname.startsWith('/v1/')) {
    // Authoritative server state & ranked outcomes are NEVER cached or faked offline.
    return false;
  }
  if (STATIC_ALLOWLIST.has(url.pathname)) {
    return true;
  }
  return (
    url.pathname.startsWith('/wasm/') ||
    url.pathname.startsWith('/icons/') ||
    url.pathname.startsWith('/assets/content/') ||
    url.pathname.endsWith('.opus') ||
    url.pathname.endsWith('.wasm')
  );
}

self.addEventListener('install', event => {
  event.waitUntil(
    caches.open(CACHE_NAME).then(cache =>
      cache.addAll(['./index.html', './manifest.json', './styles.css', './m4-shell.css']),
    ),
  );
  self.skipWaiting();
});

self.addEventListener('activate', event => {
  event.waitUntil(
    caches.keys().then(keys =>
      Promise.all(keys.filter(key => key !== CACHE_NAME).map(key => caches.delete(key))),
    ),
  );
  self.clients.claim();
});

self.addEventListener('fetch', event => {
  if (!isPermittedAssetRequest(event.request)) {
    return;
  }
  event.respondWith(
    caches.match(event.request).then(cached => {
      if (cached) {
        return cached;
      }
      return fetch(event.request).then(resp => {
        if (resp && resp.status === 200) {
          const clone = resp.clone();
          caches.open(CACHE_NAME).then(cache => cache.put(event.request, clone));
        }
        return resp;
      });
    }),
  );
});
