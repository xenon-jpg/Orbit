import { describe, expect, it } from 'vitest';
import { validateProfile } from '../../pages/Performance';
import { defaultPerformance } from '../demo';
import { text, type TextKey } from '../i18n';
describe('profile and localization contracts', () => {
  it('clamps supported values and strips arbitrary flags', () => {
    const p = validateProfile({ ...defaultPerformance, fps: 60, cpuPercent: 999, textureQuality: 99, arbitraryFlag: 'bad' });
    expect(p.cpuPercent).toBeLessThanOrEqual(100);
    expect(p.textureQuality).toBeLessThanOrEqual(3);
    expect(p).not.toHaveProperty('arbitraryFlag');
  });
  it('rejects invalid FPS', () => { expect(() => validateProfile({ ...defaultPerformance, fps: -1 })).toThrow(); });
  it('provides Russian and English navigation', () => {
    for (const key of ['home','accounts','play','servers','settings'] as TextKey[]) {
      expect(text('ru', key)).not.toBe(text('en', key));
    }
  });
});
