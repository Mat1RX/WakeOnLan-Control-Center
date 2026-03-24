const CACHE_NAME = 'wol-cache-v0.1.7';
const ASSETS_TO_CACHE = [
  './',
  './index.html',
  './styles.css',
  './app.js',
  './favicon.svg',
  './manifest.json'
];

self.addEventListener('install', (event) => {
  event.waitUntil(
      caches.open(CACHE_NAME).then((cache) => {
          return cache.addAll(ASSETS_TO_CACHE);
      })
  );
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
      caches.keys().then((cacheNames) => {
          return Promise.all(
              cacheNames.map((cacheName) => {
                  if (cacheName !== CACHE_NAME) {
                      return caches.delete(cacheName);
                  }
              })
          );
      })
  );
  self.clients.claim();
});

self.addEventListener('fetch', (event) => {
  if (event.request.method !== 'GET') return;

  // Exclude API calls from caching — always fetch live
  if (event.request.url.includes('/api/') || event.request.url.includes('/auth/login')) {
      return;
  }

  event.respondWith(
      caches.match(event.request).then((cachedResponse) => {
          const fetchPromise = fetch(event.request).then((networkResponse) => {
              if (networkResponse.ok && event.request.url.startsWith(self.location.origin)) {
                  caches.open(CACHE_NAME).then((cache) => {
                      cache.put(event.request, networkResponse.clone());
                  });
              }
              return networkResponse;
          }).catch(() => {
              return cachedResponse;
          });

          return cachedResponse || fetchPromise;
      })
  );
});
