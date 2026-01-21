import { cn } from '@/lib/utils';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';

interface ComplexityDotsProps {
  /** Complexity score from 1-100 */
  score: number;
  /** Optional className for the container */
  className?: string;
}

/**
 * Get complexity label based on score.
 */
function getComplexityLabel(score: number): string {
  if (score <= 20) return 'Simple';
  if (score <= 40) return 'Moderate';
  if (score <= 60) return 'Complex';
  if (score <= 80) return 'Very Complex';
  return 'Highly Complex';
}

/**
 * Visual complexity indicator using filled/half/empty dots.
 * Maps a 1-100 score to a 0.5-5 dot scale with half-dot precision.
 */
export function ComplexityDots({ score, className }: ComplexityDotsProps) {
  // Map 1-100 score to 0.5-5 dots in 0.5 increments
  // 1-10 = 0.5, 11-20 = 1, 21-30 = 1.5, ..., 91-100 = 5
  const dotValue = Math.min(5, Math.max(0.5, Math.ceil(score / 10) / 2));
  const fullDots = Math.floor(dotValue);
  const hasHalfDot = dotValue % 1 !== 0;

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div className={cn('flex items-center gap-0.5 cursor-default', className)}>
            {Array.from({ length: 5 }).map((_, i) => {
              const isFull = i < fullDots;
              const isHalf = i === fullDots && hasHalfDot;

              return (
                <span
                  key={i}
                  className={cn(
                    'w-1.5 h-1.5 rounded-full',
                    isFull && 'bg-foreground',
                    isHalf &&
                      'bg-linear-to-r from-foreground from-50% to-muted-foreground/30 to-50%',
                    !isFull && !isHalf && 'bg-muted-foreground/30'
                  )}
                />
              );
            })}
          </div>
        </TooltipTrigger>
        <TooltipContent>
          <p className="font-medium">{getComplexityLabel(score)}</p>
          <p className="text-xs text-muted-foreground">Complexity score: {score}/100</p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
