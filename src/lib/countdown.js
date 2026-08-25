// @ts-check

/**
 * @param {number} targetDateMs
 * @param {boolean} launcherInTray
 * @returns {boolean}
 */
export function shouldRunCountdown(targetDateMs, launcherInTray) {
 return targetDateMs > 0 && !launcherInTray;
}

/**
 * @param {number} targetDateMs
 * @param {number} nowMs
 * @returns {boolean}
 */
export function countdownExpired(targetDateMs, nowMs) {
 return targetDateMs > 0 && nowMs >= targetDateMs;
}
