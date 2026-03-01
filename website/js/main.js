// Hive — hivecode.app main JS

// Mobile menu toggle
document.addEventListener('DOMContentLoaded', () => {
    const toggle = document.querySelector('.mobile-menu-toggle');
    const links = document.querySelector('.nav-links');
    if (toggle && links) {
        toggle.addEventListener('click', () => {
            links.classList.toggle('open');
        });
    }
});

// Smooth scroll for hash links
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
        const target = document.querySelector(this.getAttribute('href'));
        if (target) {
            e.preventDefault();
            target.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
    });
});

// Navbar background on scroll
window.addEventListener('scroll', () => {
    const navbar = document.querySelector('.navbar');
    if (navbar) {
        if (window.scrollY > 20) {
            navbar.style.background = 'rgba(10, 14, 23, 0.95)';
        } else {
            navbar.style.background = 'rgba(10, 14, 23, 0.85)';
        }
    }
});
