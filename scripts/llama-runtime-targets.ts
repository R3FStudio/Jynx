export type LlamaRuntime = 'vulkan' | 'cuda' | 'rocm' | 'metal' | 'cpu';
export type LlamaPlatform = 'windows' | 'linux' | 'macos';
export type LlamaTarget = { platform: LlamaPlatform; arch: 'x64' | 'arm64'; runtime: LlamaRuntime };
type Target = LlamaTarget;

function normalizePlatform(platform: string) {
  return platform === 'win32' ? 'windows' : platform === 'darwin' ? 'macos' : platform;
}

export function defaultLlamaRuntime(platform: string, arch: string): LlamaRuntime {
  platform = normalizePlatform(platform);
  return platform === 'macos' ? (arch === 'arm64' ? 'metal' : 'cpu') : platform === 'windows' && arch === 'arm64' ? 'cpu' : 'vulkan';
}

const fullMatrix: Target[] = [
  { platform: 'windows', arch: 'x64', runtime: 'vulkan' },
  { platform: 'windows', arch: 'x64', runtime: 'cuda' },
  { platform: 'windows', arch: 'x64', runtime: 'rocm' },
  { platform: 'windows', arch: 'x64', runtime: 'cpu' },
  { platform: 'windows', arch: 'arm64', runtime: 'cuda' },
  { platform: 'windows', arch: 'arm64', runtime: 'cpu' },
  { platform: 'linux', arch: 'x64', runtime: 'vulkan' },
  { platform: 'linux', arch: 'x64', runtime: 'cuda' },
  { platform: 'linux', arch: 'x64', runtime: 'rocm' },
  { platform: 'linux', arch: 'x64', runtime: 'cpu' },
  { platform: 'linux', arch: 'arm64', runtime: 'vulkan' },
  { platform: 'linux', arch: 'arm64', runtime: 'cuda' },
  { platform: 'linux', arch: 'arm64', runtime: 'cpu' },
  { platform: 'macos', arch: 'arm64', runtime: 'metal' },
  { platform: 'macos', arch: 'arm64', runtime: 'cpu' },
  { platform: 'macos', arch: 'x64', runtime: 'cpu' },
];

export function selectLlamaTargets(args: string[], hostPlatform: string, hostArch: string): LlamaTarget[] {
  const value = (flag: string) => {
    const index = args.indexOf(flag);
    if (index < 0) return undefined;
    if (!args[index + 1] || args[index + 1].startsWith('--')) throw new Error(`Missing value for ${flag}`);
    return args[index + 1];
  };
  const platform = normalizePlatform(value('--platform') || hostPlatform);
  const arch = value('--arch') || hostArch;
  const runtime = value('--runtime');
  if (!['windows', 'linux', 'macos'].includes(platform) || !['x64', 'arm64'].includes(arch)) throw new Error(`Unsupported target: ${platform}/${arch}`);
  const explicit = ['--platform', '--arch', '--runtime', '--host', '--cross-platform', '--portable'].some((flag) => args.includes(flag));
  if (args.includes('--all') && explicit) throw new Error('Choose --all or target selectors, not both');
  if (args.includes('--cross-platform')) return fullMatrix.filter((target) => target.runtime === 'cpu' || target.runtime === defaultLlamaRuntime(target.platform, target.arch)).map((target) => ({ ...target }));
  if (!explicit) return fullMatrix.map((target) => ({ ...target }));
  if(args.includes('--portable')) {
    if(runtime)throw new Error('Choose --portable or --runtime, not both');
    return fullMatrix.filter(target=>target.platform===platform&&target.arch===arch&&(target.runtime==='cpu'||target.runtime===defaultLlamaRuntime(platform,arch))).map(target=>({...target}));
  }
  const selectedRuntime = runtime || (args.includes('--host') ? defaultLlamaRuntime(platform, arch) : undefined);
  const targets = fullMatrix.filter((target) => target.platform === platform && target.arch === arch && (!selectedRuntime || target.runtime === selectedRuntime));
  if (!targets.length) throw new Error(`Unsupported target: ${platform}/${arch}/${selectedRuntime}`);
  return targets.map((target) => ({ ...target }));
}
