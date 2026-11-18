/**
 * TrophyVault Web Showcase - Main Application Entrypoint
 * High-performance vanilla ES module dashboard for TrophyVault.
 */

import { TrophyVaultApi, formatPlaytime, getOvrTier, normalizePlatform, generateFallbackBanner } from './api.js';

class TrophyVaultApp {
  constructor() {
    this.api = new TrophyVaultApi();
    this.games = [];
    this.stats = null;
    this.filteredGames = [];
    
    // UI State
    this.selectedPlatform = 'all';
    this.searchQuery = '';
    this.currentSort = 'playtime_desc';
    this.currentView = 'grid'; // 'grid' | 'table'
    this.selectedModalGame = null;
    this.trophyFilter = 'all'; // 'all' | 'unlocked' | 'locked'

    // Share & Embed State
    this.embedTheme = 'cyberpunk';
    this.embedFormat = 'passport'; // 'passport' | 'badge' | 'single'
    this.embedAppid = '';

    // Cache DOM Elements
    this.dom = {
      apiStatusChip: document.getElementById('apiStatusChip'),
      apiStatusDot: document.getElementById('apiStatusDot'),
      apiStatusText: document.getElementById('apiStatusText'),
      personaName: document.getElementById('personaName'),
      personaInitial: document.getElementById('personaInitial'),
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

      // Hall of Fame
      hofGrid: document.getElementById('hofGrid'),
      hofCountBadge: document.getElementById('hofCountBadge'),
      hofEmptyState: document.getElementById('hofEmptyState'),

      // Library Controls
      libraryCountBadge: document.getElementById('libraryCountBadge'),
      searchInput: document.getElementById('searchInput'),
      searchClearBtn: document.getElementById('searchClearBtn'),
      platformFilters: document.getElementById('platformFilters'),
      sortSelect: document.getElementById('sortSelect'),
      viewGridBtn: document.getElementById('viewGridBtn'),
      viewTableBtn: document.getElementById('viewTableBtn'),
      gamesGrid: document.getElementById('gamesGrid'),
      gamesTableContainer: document.getElementById('gamesTableContainer'),
      gamesTableBody: document.getElementById('gamesTableBody'),
      emptyState: document.getElementById('emptyState'),
      copyCliBtn: document.getElementById('copyCliBtn'),
      emptyRetryBtn: document.getElementById('emptyRetryBtn'),

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
    };
  }

  /**
   * Initialize application and event listeners
   */
  async init() {
    this.bindEvents();
    await this.loadData();
    this.updateShareEmbedConfig();
  }

  /**
   * Bind DOM event handlers
   */
  bindEvents() {
    // Refresh button
    this.dom.refreshBtn?.addEventListener('click', () => {
      this.setApiStatus('connecting', 'FETCHING...');
      this.loadData();
    });

    // Empty state retry button
    this.dom.emptyRetryBtn?.addEventListener('click', () => {
      this.setApiStatus('connecting', 'FETCHING...');
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
      if (!btn) return;
      this.dom.platformFilters.querySelectorAll('.hud-filter-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      this.selectedPlatform = btn.dataset.platform || 'all';
      this.applyFilterAndSort();
    });

    // Sort selector
    this.dom.sortSelect?.addEventListener('change', (e) => {
      this.currentSort = e.target.value;
      this.applyFilterAndSort();
    });

    // View mode toggle
    this.dom.viewGridBtn?.addEventListener('click', () => this.setViewMode('grid'));
    this.dom.viewTableBtn?.addEventListener('click', () => this.setViewMode('table'));

    // Modals: Open Share & Embed
    this.dom.openShareModalBtn?.addEventListener('click', () => {
      this.openModal(this.dom.shareModal);
      this.updateShareEmbedConfig();
    });

    // Modals: Close handlers
    this.dom.gameModalCloseBtn?.addEventListener('click', () => this.closeModal(this.dom.gameModal));
    this.dom.shareModalCloseBtn?.addEventListener('click', () => this.closeModal(this.dom.shareModal));

    // Close modal on backdrop click
    [this.dom.gameModal, this.dom.shareModal].forEach(modal => {
      modal?.addEventListener('click', (e) => {
        if (e.target === modal) this.closeModal(modal);
      });
    });

    // Close modal on Escape key
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        if (this.dom.gameModal.style.display !== 'none') this.closeModal(this.dom.gameModal);
        if (this.dom.shareModal.style.display !== 'none') this.closeModal(this.dom.shareModal);
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
  }

  /**
   * Set API connection indicator status
   */
  setApiStatus(state, text) {
    if (!this.dom.apiStatusDot || !this.dom.apiStatusText) return;
    this.dom.apiStatusDot.className = 'hud-status-dot';
    
    if (state === 'online') {
      this.dom.apiStatusDot.classList.add('online');
      this.dom.apiStatusText.textContent = text || 'LIVE SYNCED';
    } else if (state === 'connecting') {
      this.dom.apiStatusDot.classList.add('pulse');
      this.dom.apiStatusText.textContent = text || 'CONNECTING...';
    } else {
      this.dom.apiStatusDot.classList.add('offline');
      this.dom.apiStatusText.textContent = text || 'OFFLINE';
    }
  }

  /**
   * Fetch data from TrophyVault REST endpoints
   */
  async loadData() {
    this.setApiStatus('connecting', 'SYNCING...');
    
    try {
      const [health, stats, games] = await Promise.all([
        this.api.getHealth(),
        this.api.getStats(),
        this.api.getGames()
      ]);

      const isConnected = !!(health || stats || (games && games.length >= 0));
      this.setApiStatus(isConnected ? 'online' : 'offline');

      this.games = Array.isArray(games) ? games : [];
      this.stats = stats;

      this.renderHeaderAndHero();
      this.renderHallOfFame();
      this.populateSingleGameDropdown();
      this.applyFilterAndSort();
    } catch (err) {
      console.error('[TrophyVault] Error loading data:', err);
      this.setApiStatus('offline', 'OFFLINE');
    }
  }

  /**
   * Render Header Persona & Hero Stats Banner
   */
  renderHeaderAndHero() {
    const games = this.games;
    const stats = this.stats || {};

    // Persona name
    const persona = stats.persona_name || stats.username || stats.personaName || 'MOHANDL3G';
    if (this.dom.personaName) this.dom.personaName.textContent = persona;
    if (this.dom.personaInitial) this.dom.personaInitial.textContent = persona.charAt(0).toUpperCase();

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
   * Render 100% Hall of Fame Showcase Grid
   */
  renderHallOfFame() {
    if (!this.dom.hofGrid) return;
    const completed = this.games.filter(g => g.platinum === true || g.pct === 100 || (g.unlocked === g.total && g.total > 0));

    if (this.dom.hofCountBadge) {
      this.dom.hofCountBadge.textContent = `${completed.length} TITLES`;
    }

    this.dom.hofGrid.innerHTML = '';

    if (completed.length === 0) {
      if (this.dom.hofEmptyState) this.dom.hofEmptyState.style.display = 'block';
      return;
    }

    if (this.dom.hofEmptyState) this.dom.hofEmptyState.style.display = 'none';

    completed.forEach(game => {
      const card = document.createElement('div');
      card.className = 'hud-hof-card';
      card.setAttribute('role', 'button');
      card.setAttribute('tabindex', '0');

      const bannerUrl = game.appid && /^\d+$/.test(game.appid)
        ? `https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/${game.appid}/header.jpg`
        : generateFallbackBanner(game.name, game.platform);

      card.innerHTML = `
        <div class="hud-hof-banner">
          <img class="hud-hof-banner-img" src="${bannerUrl}" alt="${this.escapeHtml(game.name)}" loading="lazy">
          <div class="hud-hof-banner-overlay"></div>
          <div class="hud-hof-plat-badge">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
            </svg>
            <span>100% PLATINUM</span>
          </div>
        </div>
        <div class="hud-hof-content">
          <h3 class="hud-hof-title" title="${this.escapeHtml(game.name)}">${this.escapeHtml(game.name)}</h3>
          <div class="hud-hof-meta-row">
            <span>${normalizePlatform(game.platform)}</span>
            <span class="hud-hof-trophy-pill">${game.unlocked || game.total}/${game.total} Trophies</span>
          </div>
        </div>
      `;

      // Fallback for broken store image
      const img = card.querySelector('.hud-hof-banner-img');
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

      this.dom.hofGrid.appendChild(card);
    });
  }

  /**
   * Apply filters and sorting to the games collection
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
      this.dom.libraryCountBadge.textContent = `${count} ${count === 1 ? 'GAME' : 'GAMES'}`;
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

      const isCompleted = game.platinum === true || game.pct === 100;
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
      const isCompleted = game.platinum === true || game.pct === 100;
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
   * Open Game Detail Modal
   */
  openGameModal(game) {
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

    this.renderModalTrophies();
    this.openModal(this.dom.gameModal);
  }

  /**
   * Render trophy manifest inside the Game Detail Modal
   */
  renderModalTrophies() {
    if (!this.dom.modalTrophiesList || !this.selectedModalGame) return;
    const game = this.selectedModalGame;
    const trophies = game.trophies || [];

    const unlockedCount = trophies.filter(t => t.unlocked_time || t.unlocked).length;
    const lockedCount = trophies.length - unlockedCount;

    if (this.dom.countAllTrophies) this.dom.countAllTrophies.textContent = trophies.length;
    if (this.dom.countUnlockedTrophies) this.dom.countUnlockedTrophies.textContent = unlockedCount;
    if (this.dom.countLockedTrophies) this.dom.countLockedTrophies.textContent = lockedCount;

    this.dom.modalTrophiesList.innerHTML = '';

    if (trophies.length === 0) {
      this.dom.modalTrophiesList.innerHTML = `
        <div style="text-align: center; padding: 32px 16px; color: var(--text-muted); background: var(--bg-secondary); border-radius: var(--radius-md);">
          <p style="font-weight: 700; color: #fff; margin-bottom: 6px;">Trophy Manifest Not Loaded</p>
          <p style="font-size: 0.8rem;">Individual trophy definitions for this title will populate upon running <code>TrophyVault --sync</code> with active credentials.</p>
        </div>
      `;
      return;
    }

    let filtered = trophies;
    if (this.trophyFilter === 'unlocked') {
      filtered = trophies.filter(t => t.unlocked_time || t.unlocked);
    } else if (this.trophyFilter === 'locked') {
      filtered = trophies.filter(t => !t.unlocked_time && !t.unlocked);
    }

    if (filtered.length === 0) {
      this.dom.modalTrophiesList.innerHTML = `
        <div style="text-align: center; padding: 24px; color: var(--text-muted);">
          No trophies match the selected filter.
        </div>
      `;
      return;
    }

    filtered.forEach(trophy => {
      const isUnlocked = !!(trophy.unlocked_time || trophy.unlocked);
      const grade = (trophy.grade || 'bronze').toLowerCase();

      const item = document.createElement('div');
      item.className = `hud-trophy-item ${isUnlocked ? 'unlocked' : 'locked'}`;

      let gradeBadgeClass = 'grade-bronze';
      let gradeLabel = 'BRZ';
      if (grade === 'platinum') { gradeBadgeClass = 'grade-platinum'; gradeLabel = 'PLAT'; }
      else if (grade === 'gold') { gradeBadgeClass = 'grade-gold'; gradeLabel = 'GLD'; }
      else if (grade === 'silver') { gradeBadgeClass = 'grade-silver'; gradeLabel = 'SLV'; }

      item.innerHTML = `
        <div class="trophy-grade-badge ${gradeBadgeClass}">${gradeLabel}</div>
        <div class="trophy-info">
          <span class="trophy-name">${this.escapeHtml(trophy.name || 'Trophy')}</span>
          <span class="trophy-desc">${this.escapeHtml(trophy.description || 'Achievement')}</span>
        </div>
        <span class="trophy-status-tag ${isUnlocked ? 'tag-unlocked' : 'tag-locked'}">
          ${isUnlocked ? 'UNLOCKED' : 'LOCKED'}
        </span>
      `;

      this.dom.modalTrophiesList.appendChild(item);
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
