import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '../../lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full text-sm font-medium tracking-tight ring-offset-background transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-60',
  {
    variants: {
      variant: {
        default:
          'bg-brand-blue-500 text-white shadow-[0_6px_20px_rgba(73,87,193,0.35)] hover:bg-brand-blue-400 active:bg-brand-blue-600 dark:hover:bg-brand-blue-400/90',
        destructive:
          'bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90 focus-visible:ring-destructive',
        outline:
          'border border-border text-blue-grey-700 bg-transparent hover:bg-blue-grey-100/70 hover:text-blue-grey-900 dark:text-blue-grey-50 dark:border-border-dark dark:hover:bg-blue-grey-800/50',
        secondary:
          'bg-blue-grey-100 text-blue-grey-900 shadow-sm hover:bg-blue-grey-200/90 dark:bg-blue-grey-700 dark:text-blue-grey-50 dark:hover:bg-blue-grey-600',
        ghost:
          'text-blue-grey-600 hover:bg-blue-grey-100/70 hover:text-blue-grey-900 dark:text-blue-grey-200 dark:hover:bg-blue-grey-800/60 dark:hover:text-white',
        link: 'text-brand-blue-600 underline-offset-4 hover:underline dark:text-brand-blue-300 font-semibold',
      },
      size: {
        default: 'h-10 px-5',
        sm: 'h-8 px-4 text-xs',
        lg: 'h-12 px-7 text-base',
        icon: 'h-10 w-10 p-0 rounded-full',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button';
    return <Comp className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props} />;
  }
);
Button.displayName = 'Button';

export { Button, buttonVariants };
