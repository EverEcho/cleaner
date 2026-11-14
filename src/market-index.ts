export const marketCategories = {
  development: '开发工具',
  browsers: '浏览器',
  productivity: '效率办公',
  communication: '沟通协作',
  media: '影音与创作',
  security: '安全工具',
} as const;

export const marketPlatforms = { linux: 'Linux', macos: 'macOS' } as const;
export type MarketPlatform = keyof typeof marketPlatforms;
export type MarketCategory = keyof typeof marketCategories;

export interface MarketApp {
  id: string;
  name: string;
  category: MarketCategory;
  description: string;
  install: Partial<Record<MarketPlatform, string>>;
  release_repo?: string;
  upstream_version?: string;
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label}格式无效`);
  }
  return value as Record<string, unknown>;
}

function labelText(value: unknown, max: number, label: string): string {
  if (typeof value !== 'string' || !value.trim() || value.length > max || /[\u0000-\u001f\u007f]/.test(value)) {
    throw new Error(`${label}无效`);
  }
  return value.trim();
}

function packageName(value: unknown, platform: MarketPlatform, id: string): string {
  const pattern = platform === 'linux'
    ? /^(?:[a-zA-Z_][a-zA-Z0-9_]*\.){2,}[a-zA-Z_][a-zA-Z0-9_-]*$/
    : /^[a-z0-9][a-z0-9-]*$/;
  if (typeof value !== 'string' || value.length > 128 || value.trim() !== value || !pattern.test(value)) {
    throw new Error(`${id} 的 ${platform} 软件包标识无效`);
  }
  return value;
}

// Used by both the desktop view and the GitHub Actions publisher.
export function parseMarketIndex(raw: string): MarketApp[] {
  if (new TextEncoder().encode(raw).length > 512 * 1024) throw new Error('应用市场索引超过大小限制');
  const index = record(JSON.parse(raw), '应用市场索引');
  if (index.schema_version !== 1 || !Array.isArray(index.apps) || index.apps.length < 1 || index.apps.length > 500) {
    throw new Error('应用市场索引版本或条目数量无效');
  }

  const ids = new Set<string>();
  return index.apps.map((value: unknown) => {
    const app = record(value, '应用条目');
    const id = labelText(app.id, 64, '应用 ID');
    if (id !== app.id || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id) || ids.has(id)) {
      throw new Error(`无效或重复的应用 ID ${id}`);
    }
    ids.add(id);
    const category = app.category;
    if (typeof category !== 'string' || !Object.prototype.hasOwnProperty.call(marketCategories, category)) {
      throw new Error(`${id} 的分类无效`);
    }
    const channels = Object.entries(record(app.install, `${id} 的安装渠道`));
    if (!channels.length) throw new Error(`${id} 缺少安装渠道`);
    const install: MarketApp['install'] = {};
    for (const [platform, target] of channels) {
      if (platform !== 'linux' && platform !== 'macos') throw new Error(`${id} 的平台无效`);
      install[platform] = packageName(target, platform, id);
    }

    const entry: MarketApp = {
      id,
      name: labelText(app.name, 80, `${id} 的名称`),
      category: category as MarketCategory,
      description: labelText(app.description, 200, `${id} 的简介`),
      install,
    };
    if (app.release_repo !== undefined) {
      const repository = labelText(app.release_repo, 140, `${id} 的上游仓库`);
      if (!/^[a-z\d][a-z\d-]*\/[a-z\d_][a-z\d._-]*$/i.test(repository)) {
        throw new Error(`${id} 的上游仓库无效`);
      }
      entry.release_repo = repository;
    }
    if (app.upstream_version !== undefined) {
      entry.upstream_version = labelText(app.upstream_version, 80, `${id} 的上游版本`);
    }
    return entry;
  });
}
