import * as React from 'react';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';

export const GraphTooltipProvider = TooltipPrimitive.Provider;
export const GraphTooltip = TooltipPrimitive.Root;
export const GraphTooltipTrigger = TooltipPrimitive.Trigger;
export const GraphTooltipPortal = TooltipPrimitive.Portal;

export const GraphTooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className: _className, sideOffset = 5, style, ...props }, ref) => (
  <TooltipPrimitive.Content
    ref={ref}
    sideOffset={sideOffset}
    style={{
      backgroundColor: '#333',
      color: 'white',
      padding: '8px 12px',
      borderRadius: 4,
      fontSize: 12,
      whiteSpace: 'pre-wrap',
      maxWidth: 300,
      zIndex: 9999,
      boxShadow: '0 2px 10px rgba(0,0,0,0.2)',
      ...style,
    }}
    {...props}
  />
));
GraphTooltipContent.displayName = TooltipPrimitive.Content.displayName;

export const GraphTooltipArrow = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Arrow>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Arrow>
>(({ style, ...props }, ref) => (
  <TooltipPrimitive.Arrow
    ref={ref}
    style={{ fill: '#333', ...style }}
    {...props}
  />
));
GraphTooltipArrow.displayName = TooltipPrimitive.Arrow.displayName;
