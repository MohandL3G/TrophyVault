/**
 * TrophyVault Web Showcase - API Client Module
 * Provides unified fetching and data normalization for TrophyVault REST endpoints.
 */

export class TrophyVaultApi {
  constructor(baseUrl = '') {
    this.baseUrl = baseUrl;
  }

  /**
   * Health check endpoint
   */
  async getHealth() {
    try {
      const res = await fetch(`${this.baseUrl}/api/health`, {
        cache: 'no-store',
        headers: { 'Accept': 'application/json' }
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch (err) {
      console.warn('[TrophyVaultApi] getHealth failed:', err);
      return null;
    }
  }

  /**
   * Fetch library statistics
   */
  async getStats() {
    try {
      const res = await fetch(`${this.baseUrl}/api/stats`, {
        cache: 'no-store',
        headers: { 'Accept': 'application/json' }
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch (err) {
      console.warn('[TrophyVaultApi] getStats failed:', err);
      return null;
    }
  }

  /**
   * Fetch games collection
   */
  async getGames() {
    try {
      const res = await fetch(`${this.baseUrl}/api/games`, {
        cache: 'no-store',
        headers: { 'Accept': 'application/json' }
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      return Array.isArray(data) ? data : (data.games || []);
    } catch (err) {
      console.warn('[TrophyVaultApi] getGames failed:', err);
      return [];
    }
  }

  /**
   * Upload custom profile avatar (PNG binary)
   */
  async uploadAvatar(blob, password) {
    const res = await fetch(`${this.baseUrl}/api/avatar`, {
      method: 'POST',
      headers: {
        'Content-Type': 'image/png',
        'X-Admin-Password': password
      },
      body: blob
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(err.message || `Upload failed (HTTP ${res.status})`);
    }
    return await res.json();
  }

  /**
   * Delete custom profile avatar
   */
  async deleteAvatar(password) {
    const res = await fetch(`${this.baseUrl}/api/avatar`, {
      method: 'DELETE',
      headers: {
        'X-Admin-Password': password
      }
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(err.message || `Delete failed (HTTP ${res.status})`);
    }
    return await res.json();
  }
}

/**
 * Format minutes into "XXh YYm"
 */
export function formatPlaytime(minutes) {
  if (!minutes || minutes <= 0) return '0h 0m';
  const h = Math.floor(minutes / 60);
  const m = Math.floor(minutes % 60);
  return `${h}h ${m}m`;
}

/**
 * Calculate dynamic OVR tier description
 */
export function getOvrTier(score) {
  if (score >= 95) return 'S-TIER • ELITE';
  if (score >= 85) return 'A-TIER • MASTER';
  if (score >= 70) return 'B-TIER • EXPERT';
  if (score >= 50) return 'C-TIER • HUNTER';
  return 'D-TIER • ROOKIE';
}

/**
 * Normalize platform name for consistent display and filtering
 */
export function normalizePlatform(platform = '') {
  const p = platform.toLowerCase();
  if (p.includes('steam') || p.includes('luatools') || p.includes('stplug')) return 'Steam';
  if (p.includes('non-steam') || p.includes('shortcut')) return 'Non-Steam';
  if (p.includes('port') || p.includes('bloodborne') || p.includes('shadps4') || p.includes('bbport')) return 'Ports';
  return platform || 'Custom';
}

/**
 * Generate fallback SVG data URL banner for games without store headers
 */
export function generateFallbackBanner(title = 'Game', platform = 'PC') {
  const safeTitle = (title || 'Game').replace(/[<>&"]/g, '');
  const safePlatform = (platform || 'PC').replace(/[<>&"]/g, '');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="460" height="215" viewBox="0 0 460 215">
    <defs>
      <linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#0c1626"/>
        <stop offset="100%" stop-color="#161b22"/>
      </linearGradient>
      <pattern id="grid" width="20" height="20" patternUnits="userSpaceOnUse">
        <path d="M 20 0 L 0 0 0 20" fill="none" stroke="rgba(0, 240, 255, 0.05)" stroke-width="1"/>
      </pattern>
    </defs>
    <rect width="100%" height="100%" fill="url(#g)"/>
    <rect width="100%" height="100%" fill="url(#grid)"/>
    <rect x="2" y="2" width="456" height="211" fill="none" stroke="rgba(0, 240, 255, 0.2)" stroke-width="1.5" rx="6"/>
    <text x="24" y="115" font-family="-apple-system, sans-serif" font-weight="900" font-size="22" fill="#f0f6fc">${safeTitle}</text>
    <text x="24" y="145" font-family="-apple-system, sans-serif" font-weight="700" font-size="12" fill="#00f0ff" letter-spacing="1.5">${safePlatform.toUpperCase()}</text>
  </svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
