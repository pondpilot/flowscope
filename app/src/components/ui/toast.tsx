import { AlertCircle, CheckCircle, Info, XCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from './button';

export type ToastType = 'error' | 'success' | 'info' | 'warning';

interface ToastProps {
  type: ToastType;
  title: string;
  message?: string;
  onClose: () => void;
  className?: string;
}

const TOAST_ICONS = {
  error: XCircle,
  success: CheckCircle,
  info: Info,
  warning: AlertCircle,
};

const TOAST_STYLES = {
  error: 'bg-destructive text-destructive-foreground',
  success: 'bg-green-600 text-white',
  info: 'bg-blue-600 text-white',
  warning: 'bg-yellow-600 text-white',
};

export function Toast({ type, title, message, onClose, className }: ToastProps) {
  const Icon = TOAST_ICONS[type];

  return (
    <div
      className={cn(
        'p-3 rounded-md shadow-lg text-sm flex items-start gap-2 animate-in slide-in-from-bottom-2',
        TOAST_STYLES[type],
        className
      )}
    >
      <Icon className="h-4 w-4 mt-0.5 shrink-0" />
      <div className="flex-1">
        <p className="font-semibold">{title}</p>
        {message && <p className="opacity-90 text-xs mt-0.5 font-mono break-all">{message}</p>}
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 w-6 p-0 opacity-50 hover:opacity-100 hover:bg-white/10"
        onClick={onClose}
      >
        &times;
      </Button>
    </div>
  );
}
