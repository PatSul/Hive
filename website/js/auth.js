// Hive — Auth page JS
// Connects to hive-cloud server for login/signup + Stripe checkout

const API_BASE = 'https://hive-cloud.fly.dev'; // Production cloud server

// Detect plan from URL params (signup.html?plan=pro)
function getSelectedPlan() {
    const params = new URLSearchParams(window.location.search);
    return params.get('plan') || 'free';
}

// Update plan message on signup page
document.addEventListener('DOMContentLoaded', () => {
    const planMsg = document.getElementById('plan-message');
    const plan = getSelectedPlan();
    if (planMsg && plan !== 'free') {
        const names = { pro: 'Pro ($8/mo)', team: 'Team ($20/seat/mo)' };
        planMsg.textContent = `Sign up for the ${names[plan] || plan} plan`;
    }
});

// Login form
const loginForm = document.getElementById('login-form');
if (loginForm) {
    loginForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const errorEl = document.getElementById('login-error');
        errorEl.style.display = 'none';

        const email = document.getElementById('email').value.trim();
        const password = document.getElementById('password').value;

        try {
            const res = await fetch(`${API_BASE}/auth/login`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email, password }),
            });

            if (!res.ok) {
                const data = await res.json().catch(() => ({}));
                throw new Error(data.error || 'Login failed');
            }

            const data = await res.json();
            // Store tokens
            localStorage.setItem('hive_access_token', data.access_token);
            localStorage.setItem('hive_refresh_token', data.refresh_token);

            // Redirect to account or dashboard
            window.location.href = 'account.html';
        } catch (err) {
            errorEl.textContent = err.message;
            errorEl.style.display = 'block';
        }
    });
}

// Signup form
const signupForm = document.getElementById('signup-form');
if (signupForm) {
    signupForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const errorEl = document.getElementById('signup-error');
        errorEl.style.display = 'none';

        const email = document.getElementById('email').value.trim();
        const password = document.getElementById('password').value;
        const confirm = document.getElementById('confirm-password').value;

        if (password !== confirm) {
            errorEl.textContent = 'Passwords do not match';
            errorEl.style.display = 'block';
            return;
        }

        try {
            // Create account
            const res = await fetch(`${API_BASE}/auth/register`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email, password }),
            });

            if (!res.ok) {
                const data = await res.json().catch(() => ({}));
                throw new Error(data.error || 'Signup failed');
            }

            const data = await res.json();
            localStorage.setItem('hive_access_token', data.access_token);
            localStorage.setItem('hive_refresh_token', data.refresh_token);

            // If a paid plan was selected, redirect to Stripe checkout
            const plan = getSelectedPlan();
            if (plan === 'pro' || plan === 'team') {
                await startCheckout(plan, data.access_token);
            } else {
                window.location.href = 'account.html';
            }
        } catch (err) {
            errorEl.textContent = err.message;
            errorEl.style.display = 'block';
        }
    });
}

// GitHub OAuth
document.getElementById('github-login')?.addEventListener('click', () => {
    window.location.href = `${API_BASE}/auth/github`;
});
document.getElementById('github-signup')?.addEventListener('click', () => {
    const plan = getSelectedPlan();
    const redirect = plan !== 'free' ? `&plan=${plan}` : '';
    window.location.href = `${API_BASE}/auth/github?${redirect}`;
});

// Stripe checkout
async function startCheckout(plan, token) {
    try {
        const res = await fetch(`${API_BASE}/billing/checkout`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${token}`,
            },
            body: JSON.stringify({
                plan,
                success_url: `${window.location.origin}/account.html?checkout=success`,
                cancel_url: `${window.location.origin}/pricing.html?checkout=cancelled`,
            }),
        });

        if (!res.ok) throw new Error('Could not create checkout session');

        const data = await res.json();
        // Redirect to Stripe Checkout
        if (data.checkout_url) {
            window.location.href = data.checkout_url;
        }
    } catch (err) {
        console.error('Checkout error:', err);
        // Fall through to account page
        window.location.href = 'account.html';
    }
}
