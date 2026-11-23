/**
 * TrophyVault Web Showcase - Main Application Entrypoint
 * High-performance vanilla ES module dashboard for TrophyVault.
 */

import { TrophyVaultApi, formatPlaytime, getOvrTier, normalizePlatform, generateFallbackBanner, cleanGameTitle } from './api.js';

const PREFS_STORAGE_KEY = 'trophyvault_web_settings';

/**
 * Format trophy unlock timestamp to localized human-readable date
 * @param {number|string|null} ts
 * @returns {string|null}
 */
function formatTrophyUnlockTime(ts) {
  if (!ts) return null;
  let dateObj = null;
  if (typeof ts === 'number') {
    const ms = ts > 1e11 ? ts : ts * 1000;
    dateObj = new Date(ms);
  } else if (typeof ts === 'string') {
    const num = Number(ts);
    if (!isNaN(num) && num > 0) {
      const ms = num > 1e11 ? num : num * 1000;
      dateObj = new Date(ms);
    } else {
      dateObj = new Date(ts);
    }
  }
  if (!dateObj || isNaN(dateObj.getTime())) return null;
  return dateObj.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric'
  });
}

class TrophyVaultApp {
  constructor() {
    this.api = new TrophyVaultApi();
    this.games = [];
    this.stats = null;
    this.filteredGames = [];
    
    // UI State with persisted defaults
    this.selectedPlatform = 'all';
    this.perfectOnly = false; // 100% only filter
    this.searchQuery = '';
    this.currentSort = 'playtime_desc';
    this.currentView = 'grid'; // 'grid' | 'table'
    this.selectedModalGame = null;
    this.trophyFilter = 'all'; // 'all' | 'unlocked' | 'locked'

    // Avatar state
    this.avatarBlobReady = false;

    // Share & Embed State
    this.embedTheme = 'cyberpunk';
    this.embedFormat = 'passport'; // 'passport' | 'badge' | 'single'
    this.embedAppid = '';

    // Real-time sync & auto-refresh
    this.eventSource = null;
    this.autoRefreshInterval = null;

    // Cache DOM Elements
    this.dom = {
      personaChip: document.getElementById('personaChip'),
      avatarTrigger: document.getElementById('avatarTrigger'),
      personaAvatarImg: document.getElementById('personaAvatarImg'),
      personaInitial: document.getElementById('personaInitial'),
      personaLabel: document.getElementById('personaLabel'),
      personaName: document.getElementById('personaName'),
      refreshBtn: document.getElementById('refreshBtn'),
      openShareModalBtn: document.getElementById('openShareModalBtn'),

      // Hero Stats
      ovrScoreVal: document.getElementById('ovrScoreVal'),
      ovrTierVal: document.getElementById('ovrTierVal'),
      statTotalGames: document.getElementById('statTotalGames'),
      statCompletions: document.getElementById('statCompletions'),
      statCompletionRate: document.getElementById('statCompletionRate'),
      statCompletionFill: document.getElementById('statCompletionFill'),
      statPlaytime: document.getElementById('statPlaytime'),

      // Library Controls & Filters
      libraryCountBadge: document.getElementById('libraryCountBadge'),
      searchInput: document.getElementById('searchInput'),
      searchClearBtn: document.getElementById('searchClearBtn'),
      platformFilters: document.getElementById('platformFilters'),
      filter100Btn: document.getElementById('filter100Btn'),
      sortSelect: document.getElementById('sortSelect'),
      viewGridBtn: document.getElementById('viewGridBtn'),
      viewTableBtn: document.getElementById('viewTableBtn'),
      gamesGrid: document.getElementById('gamesGrid'),
      gamesTableContainer: document.getElementById('gamesTableContainer'),
      gamesTableBody: document.getElementById('gamesTableBody'),
      emptyState: document.getElementById('emptyState'),
      copyCliBtn: document.getElementById('copyCliBtn'),
      emptyRetryBtn: document.getElementById('emptyRetryBtn'),

      // Avatar Modal
      avatarModal: document.getElementById('avatarModal'),
      avatarModalCloseBtn: document.getElementById('avatarModalCloseBtn'),
      avatarDropZone: document.getElementById('avatarDropZone'),
      avatarCanvas: document.getElementById('avatarCanvas'),
      avatarDropHint: document.getElementById('avatarDropHint'),
      avatarFileInput: document.getElementById('avatarFileInput'),
      avatarChooseFileBtn: document.getElementById('avatarChooseFileBtn'),
      avatarDeleteBtn: document.getElementById('avatarDeleteBtn'),
      avatarPasswordInput: document.getElementById('avatarPasswordInput'),
      avatarCancelBtn: document.getElementById('avatarCancelBtn'),
      avatarSaveBtn: document.getElementById('avatarSaveBtn'),

      // Game Modal
      gameModal: document.getElementById('gameModal'),
      gameModalCloseBtn: document.getElementById('gameModalCloseBtn'),
      modalGameBanner: document.getElementById('modalGameBanner'),
      modalGamePlatform: document.getElementById('modalGamePlatform'),
      modalGameTitle: document.getElementById('modalGameTitle'),
      modalGameAppid: document.getElementById('modalGameAppid'),
      modalGamePlaytime: document.getElementById('modalGamePlaytime'),
      modalGamePct: document.getElementById('modalGamePct'),
      modalGameTrophies: document.getElementById('modalGameTrophies'),
      modalGameStatus: document.getElementById('modalGameStatus'),
      modalGameProgressFill: document.getElementById('modalGameProgressFill'),
      modalTrophyFilterPills: document.getElementById('modalTrophyFilterPills'),
      countAllTrophies: document.getElementById('countAllTrophies'),
      countUnlockedTrophies: document.getElementById('countUnlockedTrophies'),
      countLockedTrophies: document.getElementById('countLockedTrophies'),
      modalTrophiesList: document.getElementById('modalTrophiesList'),

      // Share Modal
      shareModal: document.getElementById('shareModal'),
      shareModalCloseBtn: document.getElementById('shareModalCloseBtn'),
      previewUrlDisplay: document.getElementById('previewUrlDisplay'),
      previewSpinner: document.getElementById('previewSpinner'),
      passportLiveImg: document.getElementById('passportLiveImg'),
      themeChips: document.getElementById('themeChips'),
      embedFormatSelect: document.getElementById('embedFormatSelect'),
      singleGameGroup: document.getElementById('singleGameGroup'),
      singleGameSelect: document.getElementById('singleGameSelect'),
      ghMarkdownSnippet: document.getElementById('ghMarkdownSnippet'),
      badgeMarkdownSnippet: document.getElementById('badgeMarkdownSnippet'),
      htmlTagSnippet: document.getElementById('htmlTagSnippet'),
      directUrlSnippet: document.getElementById('directUrlSnippet'),

      // Toast
      toastContainer: document.getElementById('toastContainer'),

      // Scroll To Top
      scrollToTopBtn: document.getElementById('scrollToTopBtn'),
    };
  }

  /**
   * Initialize application, restore settings, start sync listeners and load data
   */
  async init() {
    this.loadSavedPreferences();
    this.bindEvents();
    this.initLiveEvents();
    this.initAutoRefresh();
    await this.loadData();
    this.updateShareEmbedConfig();
  }

  /**
   * Load saved filter & view settings from localStorage
   */
  loadSavedPreferences() {
    try {
      const raw = localStorage.getItem(PREFS_STORAGE_KEY);
      if (!raw) return;
      const prefs = JSON.parse(raw);
      if (typeof prefs.selectedPlatform === 'string') {
        // Reset removed filter categories if cached in preferences
        if (prefs.selectedPlatform === 'luatools' || prefs.selectedPlatform === 'non-steam') {
          this.selectedPlatform = 'all';
        } else {
          this.selectedPlatform = prefs.selectedPlatform;
        }
      }
      if (typeof prefs.perfectOnly === 'boolean') this.perfectOnly = prefs.perfectOnly;
      if (typeof prefs.currentSort === 'string') this.currentSort = prefs.currentSort;
      if (typeof prefs.currentView === 'string') this.currentView = prefs.currentView;

      // Apply restored values to DOM controls
      if (this.dom.platformFilters) {
        this.dom.platformFilters.querySelectorAll('.hud-filter-btn').forEach(btn => {
          if (btn.id === 'filter100Btn') return;
          btn.classList.toggle('active', (btn.dataset.platform || 'all') === this.selectedPlatform);
        });
      }

      if (this.dom.filter100Btn) {
        this.dom.filter100Btn.classList.toggle('active', this.perfectOnly);
        this.dom.filter100Btn.dataset.perfect = String(this.perfectOnly);
        this.dom.filter100Btn.setAttribute('aria-pressed', String(this.perfectOnly));
      }

      if (this.dom.sortSelect) {
        this.dom.sortSelect.value = this.currentSort;
      }

      if (this.dom.viewGridBtn && this.dom.viewTableBtn) {
        this.dom.viewGridBtn.classList.toggle('active', this.currentView === 'grid');
        this.dom.viewGridBtn.setAttribute('aria-pressed', String(this.currentView === 'grid'));
        this.dom.viewTableBtn.classList.toggle('active', this.currentView === 'table');
        this.dom.viewTableBtn.setAttribute('aria-pressed', String(this.currentView === 'table'));
      }
    } catch (e) {
      console.warn('[TrophyVault] Error loading saved preferences:', e);
    }
  }

  /**
   * Persist active UI settings to localStorage
   */
  savePreferences() {
    try {
      const prefs = {
        selectedPlatform: this.selectedPlatform,
        perfectOnly: this.perfectOnly,
        currentSort: this.currentSort,
        currentView: this.currentView,
      };
      localStorage.setItem(PREFS_STORAGE_KEY, JSON.stringify(prefs));
    } catch (e) {
      console.warn('[TrophyVault] Error saving preferences:', e);
    }
  }

  /**
   * Bind DOM event handlers
   */
  bindEvents() {
    // Refresh button
    this.dom.refreshBtn?.addEventListener('click', () => {
      this.loadData();
    });

    // Empty state retry button
    this.dom.emptyRetryBtn?.addEventListener('click', () => {
      this.loadData();
    });

    // Copy CLI command button in empty state
    this.dom.copyCliBtn?.addEventListener('click', () => {
      this.copyTextToClipboard('TrophyVault --sync', 'Sync command copied to clipboard!');
    });

    // Real-time search
    this.dom.searchInput?.addEventListener('input', (e) => {
      this.searchQuery = e.target.value.trim().toLowerCase();
      if (this.dom.searchClearBtn) {
        this.dom.searchClearBtn.style.display = this.searchQuery ? 'block' : 'none';
      }
      this.applyFilterAndSort();
    });

    this.dom.searchClearBtn?.addEventListener('click', () => {
      this.dom.searchInput.value = '';
      this.searchQuery = '';
      this.dom.searchClearBtn.style.display = 'none';
      this.applyFilterAndSort();
      this.dom.searchInput.focus();
    });

    // Platform filter buttons
    this.dom.platformFilters?.addEventListener('click', (e) => {
      const btn = e.target.closest('.hud-filter-btn');
      if (!btn || btn.id === 'filter100Btn') return;
      this.dom.platformFilters.querySelectorAll('.hud-filter-btn').forEach(b => {
        if (b.id !== 'filter100Btn') b.classList.remove('active');
      });
      btn.classList.add('active');
      this.selectedPlatform = btn.dataset.platform || 'all';
      this.savePreferences();
      this.applyFilterAndSort();
    });

    // 100% Only Filter Toggle Button
    this.dom.filter100Btn?.addEventListener('click', () => {
      this.perfectOnly = !this.perfectOnly;
      this.dom.filter100Btn.classList.toggle('active', this.perfectOnly);
      this.dom.filter100Btn.dataset.perfect = String(this.perfectOnly);
      this.dom.filter100Btn.setAttribute('aria-pressed', String(this.perfectOnly));
      this.savePreferences();
      this.applyFilterAndSort();
    });

    // Sort selector
    this.dom.sortSelect?.addEventListener('change', (e) => {
      this.currentSort = e.target.value;
      this.savePreferences();
      this.applyFilterAndSort();
    });

    // View mode toggle
    this.dom.viewGridBtn?.addEventListener('click', () => this.setViewMode('grid'));
    this.dom.viewTableBtn?.addEventListener('click', () => this.setViewMode('table'));

    // Avatar Trigger: Open avatar customization modal
    this.dom.avatarTrigger?.addEventListener('click', () => this.openAvatarModal());
    this.dom.avatarTrigger?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        this.openAvatarModal();
      }
    });

    // Avatar Modal Close handlers
    this.dom.avatarModalCloseBtn?.addEventListener('click', () => this.closeModal(this.dom.avatarModal));
    this.dom.avatarCancelBtn?.addEventListener('click', () => this.closeModal(this.dom.avatarModal));

    // Avatar File chooser and drag & drop
    this.dom.avatarChooseFileBtn?.addEventListener('click', () => {
      this.dom.avatarFileInput?.click();
    });
    this.dom.avatarDropZone?.addEventListener('click', (e) => {
      if (e.target.tagName !== 'BUTTON') {
        this.dom.avatarFileInput?.click();
      }
    });
    this.dom.avatarFileInput?.addEventListener('change', (e) => {
      if (e.target.files && e.target.files[0]) {
        this.handleAvatarFile(e.target.files[0]);
      }
    });
    this.dom.avatarDropZone?.addEventListener('dragover', (e) => {
      e.preventDefault();
      this.dom.avatarDropZone.classList.add('dragover');
    });
    this.dom.avatarDropZone?.addEventListener('dragleave', () => {
      this.dom.avatarDropZone.classList.remove('dragover');
    });
    this.dom.avatarDropZone?.addEventListener('drop', (e) => {
      e.preventDefault();
      this.dom.avatarDropZone.classList.remove('dragover');
      if (e.dataTransfer.files && e.dataTransfer.files[0]) {
        this.handleAvatarFile(e.dataTransfer.files[0]);
      }
    });

    // Save avatar
    this.dom.avatarSaveBtn?.addEventListener('click', () => this.saveAvatar());

    // Remove avatar
    this.dom.avatarDeleteBtn?.addEventListener('click', () => this.deleteAvatar());

    // Modals: Open Share & Embed
    this.dom.openShareModalBtn?.addEventListener('click', () => {
      this.openModal(this.dom.shareModal);
      this.updateShareEmbedConfig();
    });

    // Modals: Close handlers
    this.dom.gameModalCloseBtn?.addEventListener('click', () => this.closeModal(this.dom.gameModal));
    this.dom.shareModalCloseBtn?.addEventListener('click', () => this.closeModal(this.dom.shareModal));

    // Close modal on backdrop click
    [this.dom.gameModal, this.dom.shareModal, this.dom.avatarModal].forEach(modal => {
      modal?.addEventListener('click', (e) => {
        if (e.target === modal) this.closeModal(modal);
      });
    });

    // Close modal on Escape key
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        if (this.dom.gameModal && this.dom.gameModal.style.display !== 'none') this.closeModal(this.dom.gameModal);
        if (this.dom.shareModal && this.dom.shareModal.style.display !== 'none') this.closeModal(this.dom.shareModal);
        if (this.dom.avatarModal && this.dom.avatarModal.style.display !== 'none') this.closeModal(this.dom.avatarModal);
      }
    });

    // Trophy filter pills in Game Detail Modal
    this.dom.modalTrophyFilterPills?.addEventListener('click', (e) => {
      const pill = e.target.closest('.pill');
      if (!pill) return;
      this.dom.modalTrophyFilterPills.querySelectorAll('.pill').forEach(p => p.classList.remove('active'));
      pill.classList.add('active');
      this.trophyFilter = pill.dataset.filter || 'all';
      this.renderModalTrophies();
    });

    // Share Modal: Theme Chips
    this.dom.themeChips?.addEventListener('click', (e) => {
      const chip = e.target.closest('.hud-theme-chip');
      if (!chip) return;
      this.dom.themeChips.querySelectorAll('.hud-theme-chip').forEach(c => c.classList.remove('active'));
      chip.classList.add('active');
      this.embedTheme = chip.dataset.theme;
      this.updateShareEmbedConfig();
    });

    // Share Modal: Format selector
    this.dom.embedFormatSelect?.addEventListener('change', (e) => {
      this.embedFormat = e.target.value;
      if (this.dom.singleGameGroup) {
        this.dom.singleGameGroup.style.display = this.embedFormat === 'single' ? 'flex' : 'none';
      }
      this.updateShareEmbedConfig();
    });

    // Share Modal: Target Game selector
    this.dom.singleGameSelect?.addEventListener('change', (e) => {
      this.embedAppid = e.target.value;
      this.updateShareEmbedConfig();
    });

    // Share Modal: Copy buttons
    document.querySelectorAll('.hud-copy-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const targetId = btn.dataset.target;
        let inputEl = null;
        if (targetId === 'ghMarkdown') inputEl = this.dom.ghMarkdownSnippet;
        if (targetId === 'badgeMarkdown') inputEl = this.dom.badgeMarkdownSnippet;
        if (targetId === 'htmlTag') inputEl = this.dom.htmlTagSnippet;
        if (targetId === 'directUrl') inputEl = this.dom.directUrlSnippet;

        if (inputEl && inputEl.value) {
          this.copyTextToClipboard(inputEl.value, `Copied ${targetId} snippet!`);
        }
      });
    });

    // Scroll To Top Button
    const scrollToTopBtn = this.dom.scrollToTopBtn || document.getElementById('scrollToTopBtn');
    if (scrollToTopBtn) {
      window.addEventListener('scroll', () => {
        scrollToTopBtn.classList.toggle('is-visible', window.scrollY > 250);
      }, { passive: true });

      scrollToTopBtn.addEventListener('click', () => {
        window.scrollTo({ top: 0, behavior: 'smooth' });
      });
    }
  }

  /**
   * Connect to Server-Sent Events endpoint for real-time library updates
   */
  initLiveEvents() {
    if (typeof window === 'undefined' || typeof window.EventSource === 'undefined') {
      console.warn('[TrophyVault] EventSource not supported by browser.');
      return;
    }

    if (this.eventSource) {
      try {
        this.eventSource.close();
      } catch (_) {}
      this.eventSource = null;
    }

    try {
      const source = new EventSource('/api/events');
      this.eventSource = source;

      source.addEventListener('update', () => {
        this.loadData({ silent: true });
      });

      source.addEventListener('message', (event) => {
        if (event.data === 'refresh' || event.data === 'update') {
          this.loadData({ silent: true });
        }
      });

      source.onerror = (err) => {
        // EventSource will automatically attempt reconnection.
        // If readyState is CLOSED (2), we log without crashing.
        if (source.readyState === EventSource.CLOSED) {
          console.debug('[TrophyVault] Live events SSE closed. Background auto-refresh active.');
        } else {
          console.debug('[TrophyVault] Live events SSE reconnecting...');
        }
      };
    } catch (err) {
      console.warn('[TrophyVault] Failed to initialize live events SSE:', err);
    }
  }

  /**
   * Setup background interval for periodic auto-refresh every 30 seconds
   */
  initAutoRefresh() {
    if (this.autoRefreshInterval) {
      clearInterval(this.autoRefreshInterval);
    }
    this.autoRefreshInterval = setInterval(() => {
      this.loadData({ silent: true });
    }, 30000);
  }

  /**
   * Fetch data from TrophyVault REST endpoints
   */
  async loadData({ silent = false } = {}) {
    if (!silent && this.dom.refreshBtn) {
      this.dom.refreshBtn.classList.add('loading');
    }

    try {
      const [health, stats, games] = await Promise.all([
        this.api.getHealth(),
        this.api.getStats(),
        this.api.getGames()
      ]);

      this.games = Array.isArray(games) ? games.map(g => ({
        ...g,
        name: cleanGameTitle(g.name)
      })) : [];
      this.stats = stats;

      this.renderHeaderAndHero();
      this.populateSingleGameDropdown();
      this.applyFilterAndSort();
    } catch (err) {
      console.error('[TrophyVault] Error loading data:', err);
      if (!silent) {
        this.showToast('Failed to load library data', 'error');
      }
    } finally {
      if (!silent && this.dom.refreshBtn) {
        this.dom.refreshBtn.classList.remove('loading');
      }
    }
  }

  /**
   * Render Header Persona & Hero Stats Banner
   */
  renderHeaderAndHero() {
    const games = this.games;
    const stats = this.stats || {};

    // Persona name
    const persona = stats.username || stats.persona_name || stats.personaName || 'MOHANDL3G';
    if (this.dom.personaName) this.dom.personaName.textContent = persona;
    if (this.dom.personaInitial) this.dom.personaInitial.textContent = persona.charAt(0).toUpperCase();

    // Slogan display: default to empty/hidden if not provided or empty string
    if (this.dom.personaLabel) {
      const slogan = (stats.slogan || '').trim();
      if (slogan.length > 0) {
        this.dom.personaLabel.textContent = slogan.toUpperCase();
        this.dom.personaLabel.style.display = 'block';
      } else {
        this.dom.personaLabel.textContent = '';
        this.dom.personaLabel.style.display = 'none';
      }
    }

    // Avatar image loading with initial fallback
    if (this.dom.personaAvatarImg) {
      this.dom.personaAvatarImg.src = `/api/avatar?t=${Date.now()}`;
      this.dom.personaAvatarImg.onload = () => {
        this.dom.personaAvatarImg.style.display = 'block';
        if (this.dom.personaInitial) this.dom.personaInitial.style.display = 'none';
      };
      this.dom.personaAvatarImg.onerror = () => {
        this.dom.personaAvatarImg.style.display = 'none';
        if (this.dom.personaInitial) this.dom.personaInitial.style.display = 'block';
      };
    }

    // Stats calculation
    const totalGames = stats.total_games ?? stats.total_configs ?? games.length;
    const completedGames = stats.perfect_games ?? games.filter(g => g.platinum || g.pct === 100).length;
    
    let completionRate = 0;
    if (stats.completion_rate !== undefined) {
      completionRate = Number(stats.completion_rate);
    } else if (totalGames > 0) {
      completionRate = (completedGames / totalGames) * 100;
    }

    let playtimeMinutes = stats.total_playtime_minutes;
    if (playtimeMinutes === undefined) {
      playtimeMinutes = games.reduce((acc, g) => acc + (g.playtime_minutes || 0), 0);
    }

    // OVR score
    let ovr = stats.ovr_score;
    if (ovr === undefined || ovr === 0) {
      ovr = Math.min(99, Math.round((completionRate * 0.8) + (completedGames * 3) + 15));
    }

    // Populate DOM
    if (this.dom.ovrScoreVal) this.dom.ovrScoreVal.textContent = ovr;
    if (this.dom.ovrTierVal) this.dom.ovrTierVal.textContent = getOvrTier(ovr);
    if (this.dom.statTotalGames) this.dom.statTotalGames.textContent = totalGames;
    if (this.dom.statCompletions) this.dom.statCompletions.textContent = completedGames;
    if (this.dom.statCompletionRate) this.dom.statCompletionRate.textContent = `${completionRate.toFixed(1)}%`;
    if (this.dom.statCompletionFill) this.dom.statCompletionFill.style.width = `${Math.min(100, Math.max(0, completionRate))}%`;
    if (this.dom.statPlaytime) this.dom.statPlaytime.textContent = formatPlaytime(playtimeMinutes);
  }

  /**
   * Apply filters and sorting to the unified games collection
   */
  applyFilterAndSort() {
    let result = [...this.games];

    // Filter by platform
    if (this.selectedPlatform !== 'all') {
      result = result.filter(g => {
        const norm = normalizePlatform(g.platform).toLowerCase();
        return norm === this.selectedPlatform;
      });
    }

    // Filter by 100% Only
    if (this.perfectOnly) {
      result = result.filter(g => g.platinum === true || g.pct === 100 || (g.unlocked === g.total && g.total > 0));
    }

    // Filter by search query
    if (this.searchQuery) {
      result = result.filter(g => {
        const nameMatch = (g.name || '').toLowerCase().includes(this.searchQuery);
        const appidMatch = (g.appid || '').toString().toLowerCase().includes(this.searchQuery);
        return nameMatch || appidMatch;
      });
    }

    // Sort
    result.sort((a, b) => {
      switch (this.currentSort) {
        case 'playtime_desc':
          return (b.playtime_minutes || 0) - (a.playtime_minutes || 0);
        case 'pct_desc':
          return (b.pct || 0) - (a.pct || 0);
        case 'pct_asc':
          return (a.pct || 0) - (b.pct || 0);
        case 'name_asc':
          return (a.name || '').localeCompare(b.name || '');
        case 'unlocked_desc':
          return (b.unlocked || 0) - (a.unlocked || 0);
        default:
          return 0;
      }
    });

    this.filteredGames = result;
    this.renderLibrary();
  }

  /**
   * Render the games library in current view mode
   */
  renderLibrary() {
    const count = this.filteredGames.length;
    if (this.dom.libraryCountBadge) {
      if (this.perfectOnly) {
        this.dom.libraryCountBadge.textContent = `${count} ${count === 1 ? 'PERFECT TITLE' : 'PERFECT TITLES'}`;
      } else {
        this.dom.libraryCountBadge.textContent = `${count} ${count === 1 ? 'GAME' : 'GAMES'}`;
      }
    }

    if (this.games.length === 0 || count === 0) {
      if (this.dom.gamesGrid) this.dom.gamesGrid.style.display = 'none';
      if (this.dom.gamesTableContainer) this.dom.gamesTableContainer.style.display = 'none';
      if (this.dom.emptyState) this.dom.emptyState.style.display = 'flex';
      return;
    }

    if (this.dom.emptyState) this.dom.emptyState.style.display = 'none';

    if (this.currentView === 'grid') {
      if (this.dom.gamesGrid) this.dom.gamesGrid.style.display = 'grid';
      if (this.dom.gamesTableContainer) this.dom.gamesTableContainer.style.display = 'none';
      this.renderGamesGrid();
    } else {
      if (this.dom.gamesGrid) this.dom.gamesGrid.style.display = 'none';
      if (this.dom.gamesTableContainer) this.dom.gamesTableContainer.style.display = 'block';
      this.renderGamesTable();
    }
  }

  /**
   * Render Grid View cards
   */
  renderGamesGrid() {
    if (!this.dom.gamesGrid) return;
    this.dom.gamesGrid.innerHTML = '';

    this.filteredGames.forEach(game => {
      const card = document.createElement('div');
      card.className = 'hud-game-card';
      card.setAttribute('role', 'button');
      card.setAttribute('tabindex', '0');

      const isCompleted = game.platinum === true || game.pct === 100 || (game.unlocked === game.total && game.total > 0);
      if (isCompleted) {
        card.classList.add('is-perfect');
      }

      const bannerUrl = game.appid && /^\d+$/.test(game.appid)
        ? `https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}/header.jpg`
        : generateFallbackBanner(game.name, game.platform);

      card.innerHTML = `
        <div class="hud-card-banner">
          <img class="hud-card-banner-img" src="${bannerUrl}" alt="${this.escapeHtml(game.name)}" loading="lazy">
          <div class="hud-card-banner-overlay"></div>
          <span class="hud-card-platform-tag">${normalizePlatform(game.platform)}</span>
          ${isCompleted ? `
            <div class="hud-card-plat-badge" title="100% Platinum Completed">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
              </svg>
            </div>
          ` : ''}
        </div>
        <div class="hud-card-content">
          <h3 class="hud-card-title" title="${this.escapeHtml(game.name)}">${this.escapeHtml(game.name)}</h3>
          <div class="hud-card-stats-row">
            <span class="hud-card-playtime">${formatPlaytime(game.playtime_minutes)}</span>
            <span class="hud-card-pct">${game.pct || 0}%</span>
          </div>
          <div class="hud-card-progress">
            <div class="hud-card-progress-fill ${isCompleted ? 'completed' : ''}" style="width: ${game.pct || 0}%;"></div>
          </div>
          <div class="hud-card-footer">
            <span>Trophies</span>
            <span><strong>${game.unlocked || 0}</strong> / ${game.total || 0}</span>
          </div>
        </div>
      `;

      const img = card.querySelector('.hud-card-banner-img');
      img.addEventListener('error', () => {
        img.src = generateFallbackBanner(game.name, game.platform);
      });

      card.addEventListener('click', () => this.openGameModal(game));
      card.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          this.openGameModal(game);
        }
      });

      this.dom.gamesGrid.appendChild(card);
    });
  }

  /**
   * Render Table View rows
   */
  renderGamesTable() {
    if (!this.dom.gamesTableBody) return;
    this.dom.gamesTableBody.innerHTML = '';

    this.filteredGames.forEach(game => {
      const row = document.createElement('tr');
      const isCompleted = game.platinum === true || game.pct === 100 || (game.unlocked === game.total && game.total > 0);
      if (isCompleted) {
        row.classList.add('is-perfect');
      }

      const bannerUrl = game.appid && /^\d+$/.test(game.appid)
        ? `https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}/header.jpg`
        : generateFallbackBanner(game.name, game.platform);

      let statusBadge = '<span class="table-status-pill status-in-progress">In Progress</span>';
      if (isCompleted) {
        statusBadge = '<span class="table-status-pill status-completed">100% Complete</span>';
      } else if (!game.unlocked || game.unlocked === 0) {
        statusBadge = '<span class="table-status-pill status-not-started">Not Started</span>';
      }

      row.innerHTML = `
        <td class="col-game">
          <div class="table-game-wrap">
            <img class="table-game-thumb" src="${bannerUrl}" alt="${this.escapeHtml(game.name)}" loading="lazy">
            <span class="table-game-name">${this.escapeHtml(game.name)}</span>
          </div>
        </td>
        <td class="col-platform">${normalizePlatform(game.platform)}</td>
        <td class="col-progress">
          <div style="display: flex; align-items: center; gap: 8px;">
            <div class="hud-card-progress" style="width: 80px;">
              <div class="hud-card-progress-fill ${isCompleted ? 'completed' : ''}" style="width: ${game.pct || 0}%;"></div>
            </div>
            <span style="font-family: var(--font-mono); font-size: 0.75rem;">${game.pct || 0}%</span>
          </div>
        </td>
        <td class="col-trophies" style="font-family: var(--font-mono);">${game.unlocked || 0} / ${game.total || 0}</td>
        <td class="col-playtime" style="font-family: var(--font-mono);">${formatPlaytime(game.playtime_minutes)}</td>
        <td class="col-status">${statusBadge}</td>
        <td class="col-action">
          <button class="hud-btn hud-btn-outline" style="padding: 3px 8px; font-size: 0.72rem;">Inspect</button>
        </td>
      `;

      const img = row.querySelector('.table-game-thumb');
      img.addEventListener('error', () => {
        img.src = generateFallbackBanner(game.name, game.platform);
      });

      row.addEventListener('click', () => this.openGameModal(game));
      this.dom.gamesTableBody.appendChild(row);
    });
  }

  /**
   * Set grid vs table view mode
   */
  setViewMode(mode) {
    this.currentView = mode;
    if (this.dom.viewGridBtn) {
      this.dom.viewGridBtn.classList.toggle('active', mode === 'grid');
      this.dom.viewGridBtn.setAttribute('aria-pressed', mode === 'grid');
    }
    if (this.dom.viewTableBtn) {
      this.dom.viewTableBtn.classList.toggle('active', mode === 'table');
      this.dom.viewTableBtn.setAttribute('aria-pressed', mode === 'table');
    }
    this.renderLibrary();
  }

  /**
   * Open Game Detail Modal and fetch trophy manifest
   * @param {string|number|object} gameOrId - Game object or game identifier
   */
  async openGameModal(gameOrId) {
    let game = null;
    let gameId = null;

    if (typeof gameOrId === 'object' && gameOrId !== null) {
      game = gameOrId;
      gameId = game.id || game.appid;
    } else if (typeof gameOrId === 'string' || typeof gameOrId === 'number') {
      gameId = String(gameOrId);
      game = this.games.find(g => String(g.id) === gameId || String(g.appid) === gameId) || null;
    }

    if (!game) {
      game = {
        id: gameId,
        appid: gameId,
        name: 'Game Details',
        platform: 'Unknown',
        playtime_minutes: 0,
        pct: 0,
        unlocked: 0,
        total: 0,
        trophies: []
      };
    }

    this.selectedModalGame = game;
    this.trophyFilter = 'all';

    // Reset trophy filter pills
    if (this.dom.modalTrophyFilterPills) {
      this.dom.modalTrophyFilterPills.querySelectorAll('.pill').forEach(p => {
        p.classList.toggle('active', p.dataset.filter === 'all');
      });
    }

    // Set Header Banner
    const bannerUrl = game.appid && /^\d+$/.test(game.appid)
      ? `https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}/header.jpg`
      : generateFallbackBanner(game.name, game.platform);

    if (this.dom.modalGameBanner) {
      this.dom.modalGameBanner.style.backgroundImage = `url('${bannerUrl}')`;
    }
    if (this.dom.modalGamePlatform) {
      this.dom.modalGamePlatform.textContent = normalizePlatform(game.platform).toUpperCase();
    }
    if (this.dom.modalGameTitle) {
      this.dom.modalGameTitle.textContent = game.name;
    }
    if (this.dom.modalGameAppid) {
      this.dom.modalGameAppid.textContent = `AppID: ${game.appid || 'N/A'}`;
    }
    if (this.dom.modalGamePlaytime) {
      this.dom.modalGamePlaytime.textContent = `Playtime: ${formatPlaytime(game.playtime_minutes)}`;
    }

    // Stats bar
    if (this.dom.modalGamePct) this.dom.modalGamePct.textContent = `${game.pct || 0}%`;
    if (this.dom.modalGameTrophies) this.dom.modalGameTrophies.textContent = `${game.unlocked || 0} / ${game.total || 0}`;
    if (this.dom.modalGameStatus) {
      const isPlat = game.platinum === true || game.pct === 100;
      this.dom.modalGameStatus.textContent = isPlat ? '100% Platinum' : (game.unlocked > 0 ? 'In Progress' : 'Not Started');
      this.dom.modalGameStatus.style.color = isPlat ? 'var(--gold)' : 'var(--cyan)';
    }
    if (this.dom.modalGameProgressFill) {
      this.dom.modalGameProgressFill.style.width = `${game.pct || 0}%`;
    }

    // Open modal immediately
    this.openModal(this.dom.gameModal);

    // Initial render or loading indicator
    if (game.trophies && game.trophies.length > 0) {
      this.renderModalTrophies();
    } else {
      if (this.dom.modalTrophiesList) {
        this.dom.modalTrophiesList.innerHTML = `
          <div class="hud-trophies-loading">
            <div class="preview-spinner" style="width: 24px; height: 24px; border-width: 2px;"></div>
            <span>Loading trophy manifest...</span>
          </div>
        `;
      }
    }

    // Fetch individual trophies from GET /api/games/${gameId}/trophies
    const lookupId = game.id || game.appid || gameId;
    if (lookupId) {
      try {
        let res = await fetch(`/api/games/${encodeURIComponent(lookupId)}/trophies`, {
          cache: 'no-store',
          headers: { 'Accept': 'application/json' }
        });

        // Fallback to appid if lookupId was game.id and returned 404
        if (!res.ok && res.status === 404 && game.appid && game.appid !== lookupId) {
          res = await fetch(`/api/games/${encodeURIComponent(game.appid)}/trophies`, {
            cache: 'no-store',
            headers: { 'Accept': 'application/json' }
          });
        }

        if (res.ok) {
          const data = await res.json();
          const trophies = Array.isArray(data) ? data : (data.trophies || []);
          game.trophies = trophies;
          game.trophiesLoaded = true;

          // Update header stats with actual trophy counts if defined
          if (trophies.length > 0) {
            const unlockedCount = trophies.filter(t => Boolean(t.unlocked_time || t.unlocked)).length;
            game.total = trophies.length;
            game.unlocked = unlockedCount;
            game.pct = Math.round((unlockedCount / trophies.length) * 100);
            if (this.dom.modalGamePct) this.dom.modalGamePct.textContent = `${game.pct}%`;
            if (this.dom.modalGameTrophies) this.dom.modalGameTrophies.textContent = `${game.unlocked} / ${game.total}`;
            if (this.dom.modalGameProgressFill) this.dom.modalGameProgressFill.style.width = `${game.pct}%`;
            if (this.dom.modalGameStatus) {
              const isPlat = game.platinum === true || game.pct === 100;
              this.dom.modalGameStatus.textContent = isPlat ? '100% Platinum' : (game.unlocked > 0 ? 'In Progress' : 'Not Started');
              this.dom.modalGameStatus.style.color = isPlat ? 'var(--gold)' : 'var(--cyan)';
            }
          }
        } else {
          console.warn(`[TrophyVault] /api/games/${lookupId}/trophies returned HTTP ${res.status}`);
          if (!game.trophies) game.trophies = [];
        }
      } catch (err) {
        console.warn(`[TrophyVault] Error fetching trophies for ${lookupId}:`, err);
        if (!game.trophies) game.trophies = [];
      }
    } else {
      if (!game.trophies) game.trophies = [];
    }

    // Re-render modal trophies if this game is still the active modal
    if (this.selectedModalGame && (this.selectedModalGame.id === game.id || this.selectedModalGame.appid === game.appid)) {
      this.renderModalTrophies();
    }
  }

  /**
   * Render trophy manifest inside the Game Detail Modal
   */
  renderModalTrophies() {
    if (!this.dom.modalTrophiesList || !this.selectedModalGame) return;
    const game = this.selectedModalGame;
    const trophies = game.trophies || [];

    const unlockedCount = trophies.filter(t => Boolean(t.unlocked_time || t.unlocked)).length;
    const lockedCount = trophies.length - unlockedCount;

    if (this.dom.countAllTrophies) this.dom.countAllTrophies.textContent = trophies.length;
    if (this.dom.countUnlockedTrophies) this.dom.countUnlockedTrophies.textContent = unlockedCount;
    if (this.dom.countLockedTrophies) this.dom.countLockedTrophies.textContent = lockedCount;

    this.dom.modalTrophiesList.innerHTML = '';

    // Empty state if title has no achievements or trophies defined
    if (trophies.length === 0) {
      this.dom.modalTrophiesList.innerHTML = `
        <div class="hud-trophies-empty">
          <div class="empty-icon">🏆</div>
          <p class="empty-title">No Trophies Defined</p>
          <p class="empty-desc">This title does not have any achievement or trophy manifest entries registered.</p>
        </div>
      `;
      return;
    }

    let filtered = trophies;
    if (this.trophyFilter === 'unlocked') {
      filtered = trophies.filter(t => Boolean(t.unlocked_time || t.unlocked));
    } else if (this.trophyFilter === 'locked') {
      filtered = trophies.filter(t => !t.unlocked_time && !t.unlocked);
    }

    // Filter empty state
    if (filtered.length === 0) {
      this.dom.modalTrophiesList.innerHTML = `
        <div class="hud-trophies-empty">
          <p class="empty-title">No Trophies Found</p>
          <p class="empty-desc">No trophies match the "${this.escapeHtml(this.trophyFilter)}" filter.</p>
        </div>
      `;
      return;
    }

    filtered.forEach(trophy => {
      const isUnlocked = Boolean(trophy.unlocked_time || trophy.unlocked);
      const rawGrade = (trophy.grade || 'Bronze').toLowerCase().trim();

      let gradeClass = 'grade-bronze';
      let gradeLabel = 'Bronze';
      if (rawGrade.startsWith('p')) {
        gradeClass = 'grade-platinum';
        gradeLabel = 'Platinum';
      } else if (rawGrade.startsWith('g')) {
        gradeClass = 'grade-gold';
        gradeLabel = 'Gold';
      } else if (rawGrade.startsWith('s')) {
        gradeClass = 'grade-silver';
        gradeLabel = 'Silver';
      } else {
        gradeClass = 'grade-bronze';
        gradeLabel = 'Bronze';
      }

      const formattedDate = formatTrophyUnlockTime(trophy.unlocked_time);

      const card = document.createElement('div');
      card.className = `hud-trophy-card hud-trophy-item ${isUnlocked ? 'unlocked' : 'locked'}`;

      card.innerHTML = `
        <div class="trophy-badge-col">
          <div class="trophy-grade-badge ${gradeClass}" title="${gradeLabel} Trophy">${gradeLabel}</div>
        </div>
        <div class="trophy-info">
          <div class="trophy-top-line">
            <span class="trophy-name">${this.escapeHtml(trophy.name || trophy.title || 'Trophy')}</span>
            <span class="trophy-status-badge ${isUnlocked ? 'status-unlocked' : 'status-locked'}">
              ${isUnlocked ? `
                <svg class="trophy-status-icon check-icon" viewBox="0 0 24 24" width="13" height="13" fill="currentColor">
                  <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
                </svg>
                <span>Unlocked${formattedDate ? ` (${formattedDate})` : ''}</span>
              ` : `
                <svg class="trophy-status-icon lock-icon" viewBox="0 0 24 24" width="13" height="13" fill="currentColor">
                  <path d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z"/>
                </svg>
                <span>Locked</span>
              `}
            </span>
          </div>
          <div class="trophy-desc">${this.escapeHtml(trophy.description || 'No description available')}</div>
        </div>
      `;

      this.dom.modalTrophiesList.appendChild(card);
    });
  }

  /**
   * Populate target game selector for Single Game Embed
   */
  populateSingleGameDropdown() {
    if (!this.dom.singleGameSelect) return;
    this.dom.singleGameSelect.innerHTML = '';

    this.games.forEach((game, index) => {
      const opt = document.createElement('option');
      opt.value = game.appid || game.id || String(index);
      opt.textContent = `${game.name} (${normalizePlatform(game.platform)})`;
      this.dom.singleGameSelect.appendChild(opt);
    });

    if (this.games.length > 0 && !this.embedAppid) {
      this.embedAppid = this.games[0].appid || this.games[0].id || '';
    }
  }

  /**
   * Update Share & Embed live preview image and code snippets
   */
  updateShareEmbedConfig() {
    const origin = window.location.origin || 'http://localhost:5050';
    let path = '';

    if (this.embedFormat === 'badge') {
      path = `/api/badge.svg?theme=${this.embedTheme}`;
    } else if (this.embedFormat === 'single') {
      const appid = this.dom.singleGameSelect?.value || this.embedAppid || '';
      path = `/api/passport.svg?theme=${this.embedTheme}&mode=single&appid=${encodeURIComponent(appid)}`;
    } else {
      path = `/api/passport.svg?theme=${this.embedTheme}&mode=passport`;
    }

    const fullUrl = `${origin}${path}`;
    const homeUrl = `${origin}/`;

    if (this.dom.previewUrlDisplay) {
      this.dom.previewUrlDisplay.textContent = path;
    }

    // Update Live Image
    if (this.dom.passportLiveImg) {
      if (this.dom.previewSpinner) this.dom.previewSpinner.style.display = 'flex';
      this.dom.passportLiveImg.style.opacity = '0.3';

      const tempImg = new Image();
      tempImg.onload = () => {
        this.dom.passportLiveImg.src = fullUrl;
        this.dom.passportLiveImg.style.opacity = '1';
        if (this.dom.previewSpinner) this.dom.previewSpinner.style.display = 'none';
      };
      tempImg.onerror = () => {
        this.dom.passportLiveImg.src = fullUrl;
        this.dom.passportLiveImg.style.opacity = '1';
        if (this.dom.previewSpinner) this.dom.previewSpinner.style.display = 'none';
      };
      tempImg.src = fullUrl;
    }

    // Code Snippets
    if (this.dom.ghMarkdownSnippet) {
      this.dom.ghMarkdownSnippet.value = `[![TrophyVault](${fullUrl})](${homeUrl})`;
    }
    if (this.dom.badgeMarkdownSnippet) {
      const badgeUrl = `${origin}/api/badge.svg?theme=${this.embedTheme}`;
      this.dom.badgeMarkdownSnippet.value = `[![TrophyVault Badge](${badgeUrl})](${homeUrl})`;
    }
    if (this.dom.htmlTagSnippet) {
      this.dom.htmlTagSnippet.value = `<img src="${fullUrl}" alt="TrophyVault Passport" />`;
    }
    if (this.dom.directUrlSnippet) {
      this.dom.directUrlSnippet.value = fullUrl;
    }
  }

  /**
   * Copy string to clipboard and trigger toast
   */
  async copyTextToClipboard(text, message = 'Copied to clipboard!') {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        // Fallback for non-https or older browser contexts
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      }
      this.showToast(message, text);
    } catch (err) {
      console.warn('[TrophyVault] Clipboard write failed:', err);
      this.showToast('Failed to copy to clipboard', String(err));
    }
  }

  /**
   * Display interactive toast notification
   */
  showToast(title, body = '') {
    if (!this.dom.toastContainer) return;

    const toast = document.createElement('div');
    toast.className = 'hud-toast';
    toast.innerHTML = `
      <div class="hud-toast-icon">
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="20 6 9 17 4 12"/>
        </svg>
      </div>
      <div class="hud-toast-body">
        <span class="hud-toast-title">${this.escapeHtml(title)}</span>
        ${body ? `<span class="hud-toast-msg">${this.escapeHtml(body.slice(0, 50))}${body.length > 50 ? '...' : ''}</span>` : ''}
      </div>
    `;

    this.dom.toastContainer.appendChild(toast);

    setTimeout(() => {
      toast.classList.add('fade-out');
      setTimeout(() => toast.remove(), 300);
    }, 3000);
  }

  /**
   * Modal management
   */
  openModal(modal) {
    if (!modal) return;
    modal.style.display = 'flex';
    document.body.style.overflow = 'hidden';
  }

  closeModal(modal) {
    if (!modal) return;
    modal.style.display = 'none';
    document.body.style.overflow = '';
  }

  /**
   * Open Avatar Customization Modal
   */
  openAvatarModal() {
    this.avatarBlobReady = false;
    const canvas = this.dom.avatarCanvas;
    const ctx = canvas?.getContext('2d');
    if (canvas && ctx) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      canvas.style.display = 'none';
    }
    if (this.dom.avatarDropHint) this.dom.avatarDropHint.style.display = 'flex';
    if (this.dom.avatarFileInput) this.dom.avatarFileInput.value = '';

    // If an avatar is currently loaded on the page, draw it as the initial canvas preview
    if (this.dom.personaAvatarImg && this.dom.personaAvatarImg.style.display !== 'none' && this.dom.personaAvatarImg.complete) {
      try {
        if (canvas && ctx) {
          ctx.drawImage(this.dom.personaAvatarImg, 0, 0, 256, 256);
          canvas.style.display = 'block';
          if (this.dom.avatarDropHint) this.dom.avatarDropHint.style.display = 'none';
        }
      } catch (e) {}
    }

    // Restore cached password if available in sessionStorage
    const cachedPass = sessionStorage.getItem('trophyvault_admin_pass');
    if (this.dom.avatarPasswordInput && cachedPass) {
      this.dom.avatarPasswordInput.value = cachedPass;
    }

    this.openModal(this.dom.avatarModal);
  }

  /**
   * Process and auto-fit selected avatar file to square canvas
   */
  handleAvatarFile(file) {
    if (!file || !file.type.startsWith('image/')) {
      this.showToast('Please select a valid image file', 'error');
      return;
    }

    const reader = new FileReader();
    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        const canvas = this.dom.avatarCanvas;
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        const size = 256;
        canvas.width = size;
        canvas.height = size;

        // Auto center-crop and fit to square canvas
        const minDim = Math.min(img.width, img.height);
        const sx = (img.width - minDim) / 2;
        const sy = (img.height - minDim) / 2;

        ctx.clearRect(0, 0, size, size);
        ctx.drawImage(img, sx, sy, minDim, minDim, 0, 0, size, size);

        canvas.style.display = 'block';
        if (this.dom.avatarDropHint) this.dom.avatarDropHint.style.display = 'none';
        this.avatarBlobReady = true;
      };
      img.src = e.target.result;
    };
    reader.readAsDataURL(file);
  }

  /**
   * Upload and save custom profile avatar
   */
  async saveAvatar() {
    const password = this.dom.avatarPasswordInput?.value.trim();
    if (!password) {
      this.showToast('Please enter your Admin Password', 'error');
      this.dom.avatarPasswordInput?.focus();
      return;
    }

    if (!this.avatarBlobReady || !this.dom.avatarCanvas) {
      this.showToast('Please choose an image file first', 'error');
      return;
    }

    const saveBtn = this.dom.avatarSaveBtn;
    if (saveBtn) {
      saveBtn.disabled = true;
      saveBtn.textContent = 'Uploading...';
    }

    try {
      this.dom.avatarCanvas.toBlob(async (blob) => {
        if (!blob) {
          this.showToast('Failed to generate image blob', 'error');
          if (saveBtn) {
            saveBtn.disabled = false;
            saveBtn.textContent = 'Upload & Save Avatar';
          }
          return;
        }

        try {
          await this.api.uploadAvatar(blob, password);
          sessionStorage.setItem('trophyvault_admin_pass', password);
          
          if (this.dom.personaAvatarImg) {
            this.dom.personaAvatarImg.src = `/api/avatar?t=${Date.now()}`;
            this.dom.personaAvatarImg.style.display = 'block';
            if (this.dom.personaInitial) this.dom.personaInitial.style.display = 'none';
          }

          this.showToast('Avatar updated successfully!');
          this.closeModal(this.dom.avatarModal);
        } catch (err) {
          console.error('[TrophyVault] Upload avatar failed:', err);
          this.showToast(err.message || 'Failed to upload avatar', 'error');
        } finally {
          if (saveBtn) {
            saveBtn.disabled = false;
            saveBtn.textContent = 'Upload & Save Avatar';
          }
        }
      }, 'image/png');
    } catch (e) {
      if (saveBtn) {
        saveBtn.disabled = false;
        saveBtn.textContent = 'Upload & Save Avatar';
      }
      this.showToast(e.message || 'Error processing image', 'error');
    }
  }

  /**
   * Delete custom profile avatar
   */
  async deleteAvatar() {
    const password = this.dom.avatarPasswordInput?.value.trim();
    if (!password) {
      this.showToast('Please enter your Admin Password', 'error');
      this.dom.avatarPasswordInput?.focus();
      return;
    }

    const delBtn = this.dom.avatarDeleteBtn;
    if (delBtn) {
      delBtn.disabled = true;
      delBtn.textContent = 'Removing...';
    }

    try {
      await this.api.deleteAvatar(password);
      sessionStorage.setItem('trophyvault_admin_pass', password);

      if (this.dom.personaAvatarImg) {
        this.dom.personaAvatarImg.style.display = 'none';
      }
      if (this.dom.personaInitial) {
        this.dom.personaInitial.style.display = 'block';
      }

      this.showToast('Custom avatar removed successfully');
      this.closeModal(this.dom.avatarModal);
    } catch (err) {
      console.error('[TrophyVault] Delete avatar failed:', err);
      this.showToast(err.message || 'Failed to remove avatar', 'error');
    } finally {
      if (delBtn) {
        delBtn.disabled = false;
        delBtn.textContent = 'Remove';
      }
    }
  }

  /**
   * XML / HTML string escaping helper
   */
  escapeHtml(str) {
    if (!str) return '';
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }
}

// Instantiate and boot app on DOMContentLoaded
window.addEventListener('DOMContentLoaded', () => {
  const app = new TrophyVaultApp();
  app.init();
});
