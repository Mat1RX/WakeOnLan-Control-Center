// State
let token = localStorage.getItem('wol_token');
let apiUrl = localStorage.getItem('wol_api_url') || '';
let devices = [];

// DOM Elements
const views = {
    login: document.getElementById('login-view'),
    dashboard: document.getElementById('dashboard-view')
};

const loginForm = document.getElementById('login-form');
const apiUrlInput = document.getElementById('api-url');
const loginError = document.getElementById('login-error');
const loginBtn = document.getElementById('login-btn');
const logoutBtn = document.getElementById('logout-btn');
const refreshAllBtn = document.getElementById('refresh-all-btn');
const devicesList = document.getElementById('devices-list');
const deviceTemplate = document.getElementById('device-card-template');

// Initialize
function init() {
    apiUrlInput.value = apiUrl;
    
    if (token && apiUrl) {
        showView('dashboard');
        loadDevices();
    } else {
        showView('login');
    }

    // Event Listeners
    loginForm.addEventListener('submit', handleLogin);
    logoutBtn.addEventListener('click', handleLogout);
    refreshAllBtn.addEventListener('click', loadDevices);
}

// UI Helpers
function showView(viewName) {
    Object.values(views).forEach(v => v.classList.remove('active'));
    views[viewName].classList.add('active');
}

function updateBtnState(btn, isLoading, originalText) {
    const textSpan = btn.querySelector('.btn-text');
    const spinner = btn.querySelector('.spinner');
    
    if (isLoading) {
        btn.disabled = true;
        if (textSpan) textSpan.style.display = 'none';
        if (spinner) spinner.classList.remove('hidden');
    } else {
        btn.disabled = false;
        if (textSpan) textSpan.style.display = 'inline';
        if (spinner) spinner.classList.add('hidden');
    }
}

// API Interaction helpers
async function apiFetch(endpoint, options = {}) {
    const defaultHeaders = {
        'Content-Type': 'application/json'
    };

    if (token) {
        defaultHeaders['Authorization'] = `Bearer ${token}`;
    }

    const config = {
        ...options,
        headers: {
            ...defaultHeaders,
            ...options.headers,
        },
    };

    try {
        const response = await fetch(`${apiUrl}${endpoint}`, config);
        
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
        // Network errors or blocked mixed content will throw TypeError
        if (error.name === 'TypeError') {
            throw new Error('Connection failed. Verify API URL and check HTTPS mixed-content rules.');
        }
        throw error;
    }
}

// Handlers
async function handleLogin(e) {
    e.preventDefault();
    loginError.textContent = '';
    
    const username = document.getElementById('username').value;
    const password = document.getElementById('password').value;
    
    // Process API URL - ensure no trailing slash
    let rawUrl = apiUrlInput.value.trim();
    if (rawUrl.endsWith('/')) {
        rawUrl = rawUrl.slice(0, -1);
    }
    apiUrl = rawUrl;
    
    updateBtnState(loginBtn, true);
    
    try {
        const response = await fetch(`${apiUrl}/auth/login`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ username, password })
        });
        
        if (!response.ok) {
            throw new Error(response.status === 401 ? 'Invalid credentials' : 'Login failed');
        }
        
        const data = await response.json();
        token = data.token;
        
        localStorage.setItem('wol_token', token);
        localStorage.setItem('wol_api_url', apiUrl);
        
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
    localStorage.removeItem('wol_token');
    showView('login');
}

// Dashboard Logic
async function loadDevices() {
    // Show loading state if empty
    if (devicesList.children.length === 0 || document.querySelector('.loading-state')) {
        devicesList.innerHTML = `
            <div class="loading-state">
                <div class="spinner border-primary"></div>
                <p>Loading devices and checking status...</p>
            </div>
        `;
    }
    
    updateBtnState(refreshAllBtn, true);

    try {
        // Use Promise.all to fetch devices list and their statuses concurrently if possible,
        // but our API provides /api/devices and /api/status (which checks all)
        
        // Let's grab the device names first
        const listData = await apiFetch('/api/devices');
        devices = listData.devices || [];
        
        renderDevices(devices);
        
        // Now fetch bulk status
        const statusData = await apiFetch('/api/status');
        const statuses = statusData.devices || {};
        
        // Update UI with statuses
        devices.forEach(name => {
            updateDeviceState(name, statuses[name] || 'unknown');
        });

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
        const card = clone.querySelector('.device-card');
        
        card.dataset.name = name;
        card.querySelector('.device-name').textContent = name;
        
        // Setup buttons
        const btnWake = card.querySelector('.btn-wake');
        btnWake.addEventListener('click', () => handleWake(name, btnWake));
        
        const btnRefresh = card.querySelector('.btn-refresh');
        btnRefresh.addEventListener('click', () => handleSingleStatus(name, btnRefresh));
        
        devicesList.appendChild(card);
    });
}

function updateDeviceState(name, statusStr) {
    const card = devicesList.querySelector(`.device-card[data-name="${name}"]`);
    if (!card) return;
    
    const dot = card.querySelector('.status-dot');
    const text = card.querySelector('.status-text');
    
    dot.className = 'status-dot'; // reset classes
    
    if (statusStr === 'online') {
        dot.classList.add('online');
        text.textContent = 'Online';
    } else if (statusStr === 'offline') {
        dot.classList.add('offline');
        text.textContent = 'Offline';
    } else if (statusStr === 'waking') {
        dot.classList.add('waking');
        text.textContent = 'Sending Magic Packet...';
    } else {
        text.textContent = 'Status unknown';
    }
}

async function handleSingleStatus(name, btnElement) {
    btnElement.disabled = true;
    updateDeviceState(name, 'waking'); // visual feedback
    const originalText = devicesList.querySelector(`.device-card[data-name="${name}"] .status-text`).textContent;
    devicesList.querySelector(`.device-card[data-name="${name}"] .status-text`).textContent = "Checking...";
    
    try {
        const data = await apiFetch(`/api/status/${name}`);
        updateDeviceState(name, data.status);
    } catch (error) {
        updateDeviceState(name, 'offline');
        alert(`Failed to check status for ${name}: ${error.message}`);
    } finally {
        btnElement.disabled = false;
    }
}

async function handleWake(name, btnElement) {
    updateBtnState(btnElement, true);
    updateDeviceState(name, 'waking');
    
    try {
        // Send wake request. This endpoint blocks and waits for ping in our API implementation.
        const data = await apiFetch(`/api/wake/${name}`, {
            method: 'POST'
        });
        
        updateDeviceState(name, data.status);
    } catch (error) {
        alert(`Failed to wake ${name}: ${error.message}`);
        // Refresh status just to be sure
        handleSingleStatus(name, btnElement.nextElementSibling);
    } finally {
        updateBtnState(btnElement, false);
    }
}

// Boot up
document.addEventListener('DOMContentLoaded', init);
