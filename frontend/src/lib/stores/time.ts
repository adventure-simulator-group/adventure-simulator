// Game Time Store
import { writable, derived } from 'svelte/store';
import type { GameTime } from '$lib/spacetimedb/types';

const MONTHS = [
	'January',
	'February',
	'March',
	'April',
	'May',
	'June',
	'July',
	'August',
	'September',
	'October',
	'November',
	'December'
];

function createTimeStore() {
	const { subscribe, set, update } = writable<GameTime>({
		day: 23,
		month: 'August',
		year: 1544,
		hour: 15,
		minute: 35
	});

	return {
		subscribe,

		// Set full time
		setTime: (time: GameTime) => set(time),

		// Update time
		updateTime: (changes: Partial<GameTime>) =>
			update((t) => ({ ...t, ...changes })),

		// Advance time by minutes
		advanceMinutes: (minutes: number) =>
			update((t) => {
				let { minute, hour, day, month, year } = t;

				minute += minutes;

				while (minute >= 60) {
					minute -= 60;
					hour++;
				}

				while (hour >= 24) {
					hour -= 24;
					day++;
				}

				// Simple month handling (assume 30 days per month)
				while (day > 30) {
					day -= 30;
					const monthIndex = MONTHS.indexOf(month);
					if (monthIndex === 11) {
						month = MONTHS[0];
						year++;
					} else {
						month = MONTHS[monthIndex + 1];
					}
				}

				return { day, month, year, hour, minute };
			}),

		// Advance time by hours
		advanceHours: (hours: number) =>
			update((t) => {
				let { hour, day, month, year } = t;

				hour += hours;

				while (hour >= 24) {
					hour -= 24;
					day++;
				}

				while (day > 30) {
					day -= 30;
					const monthIndex = MONTHS.indexOf(month);
					if (monthIndex === 11) {
						month = MONTHS[0];
						year++;
					} else {
						month = MONTHS[monthIndex + 1];
					}
				}

				return { ...t, hour, day, month, year };
			}),

		// Advance time by days
		advanceDays: (days: number) =>
			update((t) => {
				let { day, month, year } = t;

				day += days;

				while (day > 30) {
					day -= 30;
					const monthIndex = MONTHS.indexOf(month);
					if (monthIndex === 11) {
						month = MONTHS[0];
						year++;
					} else {
						month = MONTHS[monthIndex + 1];
					}
				}

				return { ...t, day, month, year };
			}),

		// Reset to default
		reset: () =>
			set({
				day: 23,
				month: 'August',
				year: 1544,
				hour: 15,
				minute: 35
			})
	};
}

export const gameTime = createTimeStore();

// Helper to get ordinal suffix
function getOrdinalSuffix(n: number): string {
	const s = ['th', 'st', 'nd', 'rd'];
	const v = n % 100;
	return s[(v - 20) % 10] || s[v] || s[0];
}

// Derived stores
export const formattedDate = derived(gameTime, ($t) => {
	return `${$t.day}${getOrdinalSuffix($t.day)} of ${$t.month}`;
});

export const formattedTime = derived(gameTime, ($t) => {
	const hour12 = $t.hour % 12 || 12;
	const period = $t.hour >= 12 ? 'PM' : 'AM';
	const minuteStr = String($t.minute).padStart(2, '0');
	return `${hour12}:${minuteStr} ${period}`;
});

export const formattedDateTime = derived(gameTime, ($t) => {
	const hour12 = $t.hour % 12 || 12;
	const period = $t.hour >= 12 ? 'PM' : 'AM';
	const minuteStr = String($t.minute).padStart(2, '0');
	return `${$t.day}${getOrdinalSuffix($t.day)} of ${$t.month}, ${hour12}:${minuteStr} ${period}`;
});

export const isDay = derived(gameTime, ($t) => $t.hour >= 6 && $t.hour < 20);

export const isNight = derived(gameTime, ($t) => $t.hour < 6 || $t.hour >= 20);

export const timeOfDay = derived(gameTime, ($t) => {
	if ($t.hour >= 5 && $t.hour < 8) return 'dawn';
	if ($t.hour >= 8 && $t.hour < 12) return 'morning';
	if ($t.hour >= 12 && $t.hour < 14) return 'noon';
	if ($t.hour >= 14 && $t.hour < 18) return 'afternoon';
	if ($t.hour >= 18 && $t.hour < 21) return 'evening';
	return 'night';
});
