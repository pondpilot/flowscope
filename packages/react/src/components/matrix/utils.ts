import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function getShortName(name: string): string {
  if (name.endsWith('.sql')) {
    const normalized = name.replace(/\\/g, '/');
    const lastSlash = normalized.lastIndexOf('/');
    const fileName = lastSlash !== -1 ? normalized.slice(lastSlash + 1) : normalized;
    return fileName.slice(0, -4);
  }
  const lastDot = name.lastIndexOf('.');
  if (lastDot !== -1) {
    return name.slice(lastDot + 1);
  }
  return name;
}
