// Theme Toggle
const themeToggle = document.getElementById('theme-toggle');
const html = document.documentElement;

// Get saved theme or default to dark
const currentTheme = localStorage.getItem('theme') || 'dark';
html.setAttribute('data-theme', currentTheme);

// Update navbar background based on theme
function updateNavbarBackground() {
    const navbar = document.querySelector('.navbar');
    const theme = html.getAttribute('data-theme');
    if (theme === 'light') {
        navbar.style.background = 'rgba(255, 255, 255, 0.8)';
    } else {
        navbar.style.background = 'rgba(0, 0, 0, 0.7)';
    }
}

updateNavbarBackground();

themeToggle.addEventListener('click', () => {
    const currentTheme = html.getAttribute('data-theme');
    const newTheme = currentTheme === 'dark' ? 'light' : 'dark';
    
    html.setAttribute('data-theme', newTheme);
    localStorage.setItem('theme', newTheme);
    updateNavbarBackground();
});

// Smooth scrolling for anchor links
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
        e.preventDefault();
        const target = document.querySelector(this.getAttribute('href'));
        if (target) {
            target.scrollIntoView({
                behavior: 'smooth',
                block: 'start'
            });
        }
    });
});

// Add scroll effect to navbar
let lastScroll = 0;
const navbar = document.querySelector('.navbar');

window.addEventListener('scroll', () => {
    const currentScroll = window.pageYOffset;
    const theme = html.getAttribute('data-theme');
    
    if (currentScroll > 100) {
        if (theme === 'light') {
            navbar.style.background = 'rgba(255, 255, 255, 0.95)';
        } else {
            navbar.style.background = 'rgba(0, 0, 0, 0.95)';
        }
    } else {
        if (theme === 'light') {
            navbar.style.background = 'rgba(255, 255, 255, 0.8)';
        } else {
            navbar.style.background = 'rgba(0, 0, 0, 0.7)';
        }
    }
    
    lastScroll = currentScroll;
});

// Intersection Observer for fade-in animations
const observerOptions = {
    threshold: 0.1,
    rootMargin: '0px 0px -50px 0px'
};

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.style.opacity = '1';
            entry.target.style.transform = 'translateY(0)';
        }
    });
}, observerOptions);

// Observe feature cards
document.querySelectorAll('.feature-card').forEach(card => {
    card.style.opacity = '0';
    card.style.transform = 'translateY(20px)';
    card.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
    observer.observe(card);
});

// Observe architecture diagram
const archDiagram = document.querySelector('.architecture-diagram');
if (archDiagram) {
    archDiagram.style.opacity = '0';
    archDiagram.style.transform = 'translateY(20px)';
    archDiagram.style.transition = 'opacity 0.8s ease, transform 0.8s ease';
    observer.observe(archDiagram);
}
