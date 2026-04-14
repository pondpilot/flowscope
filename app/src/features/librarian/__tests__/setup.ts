import { expect } from 'vitest';
import * as matchers from '@testing-library/jest-dom/matchers';

expect.extend(matchers);

// jsdom doesn't implement scrollIntoView
Element.prototype.scrollIntoView = () => {};
