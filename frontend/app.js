// =============================================================================
// State
// =============================================================================
let token = sessionStorage.getItem('wol_token');   // Short-lived storage (tab-scoped)
let apiUrl = localStorage.getItem('wol_api_url') || '';
let devices = [];

// =============================================================================
// DOM Elements
// =============================================================================
const views = {
    login: document.getElementById('login-view'),
    dashboard: document.getElementById('dashboard-view'),
};

const loginForm       = document.getElementById('login-form');
const apiUrlInput     = document.getElementById('api-url');
const loginError      = document.getElementById('login-error');
const loginBtn        = document.getElementById('login-btn');
const logoutBtn       = document.getElementById('logout-btn');
const refreshAllBtn   = document.getElementById('refresh-all-btn');
const devicesList     = document.getElementById('devices-list');
const deviceTemplate  = document.getElementById('device-card-template');

// User Menu
const userMenuBtn     = document.getElementById('user-menu-btn');
const userDropdown    = document.getElementById('user-dropdown');
const displayUsername = document.getElementById('display-username');
const displayApiUrl   = document.getElementById('display-api-url');

// =============================================================================
// Initialization
// =============================================================================
async function init() {
    apiUrlInput.value = apiUrl;

    if (token && apiUrl) {
        const username = localStorage.getItem('wol_username') || 'User';
        if (displayUsername) displayUsername.textContent = username;
        if (displayApiUrl)   displayApiUrl.textContent = apiUrl;

        showView('dashboard');
        // Await refresh before loading devices to prevent JTI race issues
        await refreshToken(); 
        loadDevices();
    } else {
        showView('login');
    }

    loginForm.addEventListener('submit', handleLogin);
    logoutBtn.addEventListener('click', handleLogout);
    refreshAllBtn.addEventListener('click', loadDevices);

    // User menu toggle
    if (userMenuBtn && userDropdown) {
        userMenuBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            const isOpen = !userDropdown.classList.contains('hidden');
            userDropdown.classList.toggle('hidden', isOpen);
            userMenuBtn.classList.toggle('open', !isOpen);
        });

        document.addEventListener('click', (e) => {
            if (!userMenuBtn.contains(e.target) && !userDropdown.contains(e.target)) {
                userDropdown.classList.add('hidden');
                userMenuBtn.classList.remove('open');
            }
        });
    }
}

// =============================================================================
// UI Helpers
// =============================================================================
function showView(viewName) {
    Object.values(views).forEach(v => v.classList.remove('active'));
    views[viewName].classList.add('active');
}

function updateBtnState(btn, isLoading) {
    const textSpan = btn.querySelector('.btn-text');
    const spinner  = btn.querySelector('.spinner');
    btn.disabled = isLoading;
    if (textSpan) textSpan.style.display = isLoading ? 'none' : 'inline';
    if (spinner)  spinner.classList.toggle('hidden', !isLoading);
}

/**
 * Shows a floating toast notification.
 * @param {string} message
 * @param {'error'|'success'|'warning'} type
 */
function showToast(message, type = 'error') {
    const container = document.getElementById('toast-container');
    if (!container) return;

    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;

    container.appendChild(toast);

    // Trigger animation
    requestAnimationFrame(() => toast.classList.add('toast-visible'));

    setTimeout(() => {
        toast.classList.remove('toast-visible');
        toast.addEventListener('transitionend', () => toast.remove(), { once: true });
    }, 5000);
}

// =============================================================================
// API
// =============================================================================
async function apiFetch(endpoint, options = {}) {
    const headers = {
        'Content-Type': 'application/json',
        ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
        ...options.headers,
    };

    try {
        const response = await fetch(`${apiUrl}${endpoint}`, { ...options, headers });

        if (response.status === 401 && endpoint !== '/auth/login') {
            handleLogout();
            throw new Error('Session expired');
        }

        const data = await response.json();

        if (!response.ok) {
            throw new Error(data.message || `Error ${response.status}`);
        }

        return data;
    } catch (error) {
        if (error.name === 'TypeError') {
            throw new Error('Connection failed. Verify API URL and check HTTPS mixed-content rules.');
        }
        throw error;
    }
}

// =============================================================================
// Auth Handlers
// =============================================================================
async function handleLogin(e) {
    e.preventDefault();
    loginError.textContent = '';

    const username = document.getElementById('username').value;
    const password = document.getElementById('password').value;

    // Strip trailing slash from API URL
    apiUrl = apiUrlInput.value.trim().replace(/\/$/, '');

    updateBtnState(loginBtn, true);

    try {
        const response = await fetch(`${apiUrl}/auth/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ username, password }),
        });

        if (!response.ok) {
            throw new Error(response.status === 401 ? 'Invalid credentials' : 'Login failed');
        }

        const data = await response.json();
        token = data.token;

        // Token stored in sessionStorage (tab-scoped, not readable by other tabs)
        sessionStorage.setItem('wol_token', token);
        localStorage.setItem('wol_api_url', apiUrl);
        localStorage.setItem('wol_username', username);

        if (displayUsername) displayUsername.textContent = username;
        if (displayApiUrl)   displayApiUrl.textContent = apiUrl;

        showView('dashboard');
        loadDevices();
    } catch (error) {
        loginError.textContent = error.message === 'Failed to fetch'
            ? 'Connection failed. Ensure API URL uses exactly http:// or https:// as your backend prescribes.'
            : error.message;
    } finally {
        updateBtnState(loginBtn, false);
    }
}

function handleLogout() {
    token = null;
    sessionStorage.removeItem('wol_token');
    showView('login');
}

async function refreshToken() {
    if (!token || !apiUrl) return;
    try {
        const response = await fetch(`${apiUrl}/api/refresh`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${token}`,
            },
        });

        if (response.ok) {
            const data = await response.json();
            token = data.token;
            sessionStorage.setItem('wol_token', token);
        } else {
            console.warn('Silent token refresh failed, status:', response.status);
        }
    } catch (error) {
        console.error('Failed to refresh token in background:', error);
    }
}

// =============================================================================
// Dashboard
// =============================================================================
async function loadDevices() {
    if (devicesList.children.length === 0 || document.querySelector('.loading-state')) {
        devicesList.innerHTML = `
            <div class="loading-state">
                <div class="spinner"></div>
                <p>Loading devices and checking status...</p>
            </div>
        `;
    }

    updateBtnState(refreshAllBtn, true);

    try {
        const listData = await apiFetch('/api/devices');
        devices = listData.devices || [];
        renderDevices(devices);

        const statusData = await apiFetch('/api/status');
        const statuses = statusData.devices || {};
        devices.forEach(name => updateDeviceState(name, statuses[name] || 'unknown'));
    } catch (error) {
        console.error('Failed to load devices:', error);
        devicesList.innerHTML = `<div class="loading-state error-message">Failed to load: ${error.message}</div>`;
    } finally {
        updateBtnState(refreshAllBtn, false);
    }
}

function renderDevices(deviceNames) {
    if (deviceNames.length === 0) {
        devicesList.innerHTML = `<div class="loading-state"><p>No devices configured in backend.</p></div>`;
        return;
    }

    devicesList.innerHTML = '';

    deviceNames.forEach(name => {
        const clone = document.importNode(deviceTemplate.content, true);
        const card  = clone.querySelector('.device-card');

        card.dataset.name = name;
        card.querySelector('.device-name').textContent = name;

        const btnWake    = card.querySelector('.btn-wake');
        const btnRefresh = card.querySelector('.btn-refresh');

        btnWake.addEventListener('click', () => handleWake(name, btnWake));
        btnRefresh.addEventListener('click', () => handleSingleStatus(name, btnRefresh));

        devicesList.appendChild(card);
    });
}

function updateDeviceState(name, statusStr) {
    const card = devicesList.querySelector(`.device-card[data-name="${name}"]`);
    if (!card) return;

    const dot  = card.querySelector('.status-dot');
    const text = card.querySelector('.status-text');

    dot.className = 'status-dot';

    const states = {
        online:  { cls: 'online',  label: 'Online' },
        offline: { cls: 'offline', label: 'Offline' },
        waking:  { cls: 'waking',  label: 'Sending Magic Packet...' },
    };

    const state = states[statusStr];
    if (state) {
        dot.classList.add(state.cls);
        text.textContent = state.label;
    } else {
        text.textContent = 'Status unknown';
    }
}

async function handleSingleStatus(name, btnElement) {
    btnElement.disabled = true;
    const statusText = devicesList.querySelector(`.device-card[data-name="${name}"] .status-text`);
    const prevText   = statusText?.textContent;
    if (statusText) statusText.textContent = 'Checking...';

    try {
        const data = await apiFetch(`/api/status/${name}`);
        updateDeviceState(name, data.status);
    } catch (error) {
        updateDeviceState(name, 'offline');
        if (statusText && prevText) statusText.textContent = prevText;
        showToast(`Failed to check status for ${name}: ${error.message}`);
    } finally {
        btnElement.disabled = false;
    }
}

async function handleWake(name, btnElement) {
    updateBtnState(btnElement, true);
    updateDeviceState(name, 'waking');

    try {
        const data = await apiFetch(`/api/wake/${name}`, { method: 'POST' });
        updateDeviceState(name, data.status);
    } catch (error) {
        showToast(`Failed to wake ${name}: ${error.message}`);
        const btnRefresh = btnElement.nextElementSibling;
        if (btnRefresh) handleSingleStatus(name, btnRefresh);
    } finally {
        updateBtnState(btnElement, false);
    }
}

// =============================================================================
// Boot
// =============================================================================
document.addEventListener('DOMContentLoaded', async () => {
    init();

    // Only register SW if the protocol is HTTP or HTTPS
    const isSecureContext = window.location.protocol === 'http:' || window.location.protocol === 'https:';
    if ('serviceWorker' in navigator && isSecureContext) {
        window.addEventListener('load', () => {
            navigator.serviceWorker.register('./sw.js').catch(err => {
                console.error('Service Worker registration failed:', err);
            });
        });
    }
});
