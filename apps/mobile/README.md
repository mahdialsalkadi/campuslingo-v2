# CampusLingo Mobile Shell (Capacitor Android / iOS)

Thin native container wrapping `../../packages/game-client/public` for iOS App Store / Google Play delivery alongside the installable iOS Safari PWA.

- Safe-area insets (`env(safe-area-inset-*)`) and `viewport-fit=cover` are enforced in `packages/game-client/public/index.html` and `m4-shell.css`.
- Service worker (`sw.js`) caches only permitted static shell/WASM/content `.opus` assets and never queues offline ranked outcomes as trusted wins.
- Native APNs/FCM device tokens register via `POST /v1/notifications/devices` and respect `MAX_PUSH_NOTIFICATIONS_PER_DAY = 2` and `22:00–08:00` local quiet hours.
