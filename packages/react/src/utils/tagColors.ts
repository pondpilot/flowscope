const TAG_COLOR_PALETTE = ['#DB2777', '#2563EB', '#0F766E', '#9333EA', '#B45309', '#0EA5E9'];

export function getTagColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash * 31 + name.charCodeAt(i)) & 0xffffffff;
  }
  const index = Math.abs(hash) % TAG_COLOR_PALETTE.length;
  return TAG_COLOR_PALETTE[index];
}
