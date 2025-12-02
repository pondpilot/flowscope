/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
    '../packages/react/src/**/*.{js,ts,jsx,tsx}',
  ],
  darkMode: 'class',
  theme: {
  	screens: {
  		xs: '36em',
  		sm: '48em',
  		md: '62em',
  		lg: '75em',
  		xl: '88em'
  	},
  	extend: {
  		fontFamily: {
  			mono: [
  				'IBM Plex Mono',
  				'monospace'
  			],
  			sans: [
  				'Helvetica Neue',
  				'Helvetica',
  				'-apple-system',
  				'BlinkMacSystemFont',
  				'Segoe UI',
  				'sans-serif'
  			]
  		},
  		fontSize: {
  			'xs': ['12px', { lineHeight: '1.3' }],
  			'sm': ['14px', { lineHeight: '1.286' }],
  			'base': ['16px', { lineHeight: '1.3' }],
  			'lg': ['18px', { lineHeight: '1.3' }],
  			'xl': ['20px', { lineHeight: '1.3' }],
  			'2xl': ['24px', { lineHeight: '1.3' }],
  			'3xl': ['32px', { lineHeight: '1.3' }],
  		},
  		colors: {
  			border: {
  				light: '#E5E9F2',
  				dark: '#3D4654',
  				primary: {
  					light: '#CDD1D9',
  					dark: '#5B6B86'
  				},
  				accent: {
  					light: '#4957C1',
  					dark: '#4C61FF'
  				},
  				DEFAULT: 'hsl(var(--border))'
  			},
  			background: {
  				DEFAULT: 'hsl(var(--background))',
  				primary: {
  					light: '#FDFDFD',
  					dark: '#242B35'
  				},
  				secondary: {
  					light: '#F2F4F8',
  					dark: '#191D24'
  				},
  				tertiary: {
  					light: '#E5E9F2',
  					dark: '#5B6B86'
  				}
  			},
  			foreground: {
  				DEFAULT: 'hsl(var(--foreground))'
  			},
  			text: {
  				primary: {
  					light: '#212328',
  					dark: '#FDFDFD'
  				},
  				secondary: {
  					light: '#6F7785',
  					dark: '#A8B3C4'
  				},
  				tertiary: {
  					light: '#AEB2BB',
  					dark: '#8292AA'
  				}
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))',
  				light: '#4957C1',
  				dark: '#4C61FF'
  			},
  			'brand-blue': {
  				'50': '#F4F4FB',
  				'100': '#E5E7F6',
  				'200': '#B8BDE7',
  				'300': '#98A0DC',
  				'400': '#737ECF',
  				'500': '#4957C1',
  				'600': '#737ECF',
  				'700': '#26349E',
  				'800': '#252E6D',
  				'900': '#131738'
  			},
  			'blue-grey': {
  				'50': '#FFFFFF',
  				'100': '#F2F4F8',
  				'200': '#E5E9F2',
  				'300': '#C8CED9',
  				'400': '#A8B3C4',
  				'500': '#8292AA',
  				'600': '#5B6B86',
  				'700': '#384252',
  				'800': '#242B35',
  				'900': '#191D24'
  			},
  			grey: {
  				'50': '#FDFDFD',
  				'100': '#F6F6F7',
  				'200': '#EBECEE',
  				'300': '#DBDDE1',
  				'400': '#C7CAD0',
  				'500': '#AEB2BB',
  				'600': '#9096A3',
  				'700': '#6F7785',
  				'800': '#3E434B',
  				'900': '#212328'
  			},
  			success: {
  				light: '#4CAE4F',
  				dark: '#75C277'
  			},
  			warning: {
  				light: '#F4A462',
  				dark: '#F7B987'
  			},
  			error: {
  				light: '#EF486F',
  				dark: '#F37391'
  			},
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			highlight: {
  				DEFAULT: 'hsl(var(--highlight))',
  				foreground: 'hsl(var(--highlight-foreground))'
  			},
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))'
  		},
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		},
  		keyframes: {
  			'accordion-down': {
  				from: {
  					height: '0'
  				},
  				to: {
  					height: 'var(--radix-accordion-content-height)'
  				}
  			},
  			'accordion-up': {
  				from: {
  					height: 'var(--radix-accordion-content-height)'
  				},
  				to: {
  					height: '0'
  				}
  			}
  		},
  		animation: {
  			'accordion-down': 'accordion-down 0.2s ease-out',
  			'accordion-up': 'accordion-up 0.2s ease-out'
  		},
  		transitionTimingFunction: {
  			'pondpilot': 'cubic-bezier(0.4, 0, 0.2, 1)'
  		},
  		transitionDuration: {
  			'DEFAULT': '200ms'
  		}
  	}
  },
  plugins: [require('tailwindcss-animate')],
};
