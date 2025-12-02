import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap text-sm font-medium transition-all duration-200 ease-pondpilot focus-visible:outline-none disabled:pointer-events-none disabled:opacity-60 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "rounded-full bg-primary text-primary-foreground hover:bg-primary/90 border border-transparent focus-visible:border-accent-light dark:focus-visible:border-accent-dark",
        destructive:
          "rounded-full bg-destructive text-destructive-foreground hover:bg-destructive/90 border border-transparent focus-visible:border-error-light dark:focus-visible:border-error-dark",
        outline:
          "rounded-full border border-border-primary-light dark:border-border-primary-dark bg-background hover:border-accent-light dark:hover:border-accent-dark hover:text-accent-light dark:hover:text-accent-dark focus-visible:border-accent-light dark:focus-visible:border-accent-dark",
        secondary:
          "rounded-md bg-secondary text-secondary-foreground hover:bg-secondary/80 border border-transparent focus-visible:border-accent-light dark:focus-visible:border-accent-dark",
        ghost: "rounded-md hover:bg-accent/10 hover:text-accent-light dark:hover:text-accent-dark focus-visible:bg-accent/10",
        link: "text-primary underline-offset-4 hover:underline focus-visible:underline",
      },
      size: {
        default: "h-[34px] px-6 py-2",
        sm: "h-[26px] px-4 text-sm",
        lg: "h-11 px-8",
        icon: "h-10 w-10 rounded-full",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button"
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = "Button"

export { Button, buttonVariants }
