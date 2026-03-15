// Mobile hamburger menu toggle
document.addEventListener('DOMContentLoaded', function () {
    var hamburger = document.querySelector('.nav-hamburger');
    var navMenu = document.querySelector('.nav-menu');
    if (hamburger && navMenu) {
        hamburger.addEventListener('click', function () {
            hamburger.classList.toggle('active');
            navMenu.classList.toggle('open');
        });
        navMenu.querySelectorAll('a').forEach(function (link) {
            link.addEventListener('click', function () {
                hamburger.classList.remove('active');
                navMenu.classList.remove('open');
            });
        });
    }
});

// Smooth scrolling for anchor links
document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function (e) {
        const href = this.getAttribute('href');
        if (href === '#') return;
        
        e.preventDefault();
        const target = document.querySelector(href);
        if (target) {
            const navHeight = document.querySelector('.navbar').offsetHeight;
            const targetPosition = target.offsetTop - navHeight;
            
            window.scrollTo({
                top: targetPosition,
                behavior: 'smooth'
            });
        }
    });
});

// Tab switching functionality
const tabButtons = document.querySelectorAll('.tab-button');
const codePanels = document.querySelectorAll('.code-panel');

tabButtons.forEach(button => {
    button.addEventListener('click', () => {
        const targetTab = button.getAttribute('data-tab');
        
        // Remove active class from all buttons and panels
        tabButtons.forEach(btn => btn.classList.remove('active'));
        codePanels.forEach(panel => panel.classList.remove('active'));
        
        // Add active class to clicked button and corresponding panel
        button.classList.add('active');
        document.getElementById(targetTab).classList.add('active');
    });
});

// Copy to clipboard functionality
function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        // Use the Clipboard API
        navigator.clipboard.writeText(text).then(() => {
            showCopyFeedback();
        }).catch(err => {
            console.error('Failed to copy:', err);
            fallbackCopy(text);
        });
    } else {
        // Fallback for older browsers or non-secure contexts
        fallbackCopy(text);
    }
}

function fallbackCopy(text) {
    const textArea = document.createElement('textarea');
    textArea.value = text;
    textArea.style.position = 'fixed';
    textArea.style.left = '-999999px';
    textArea.style.top = '-999999px';
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    
    try {
        document.execCommand('copy');
        showCopyFeedback();
    } catch (err) {
        console.error('Failed to copy:', err);
    }
    
    textArea.remove();
}

function showCopyFeedback() {
    const copyBtn = document.querySelector('.copy-btn');
    const originalText = copyBtn.textContent;
    copyBtn.textContent = 'Copied!';
    copyBtn.style.background = 'linear-gradient(135deg, #10b981 0%, #059669 100%)';
    
    setTimeout(() => {
        copyBtn.textContent = originalText;
        copyBtn.style.background = '';
    }, 2000);
}

// Navbar background on scroll
let lastScroll = 0;
const navbar = document.querySelector('.navbar');

window.addEventListener('scroll', () => {
    const currentScroll = window.pageYOffset;
    
    if (currentScroll > 50) {
        navbar.style.background = 'rgba(10, 10, 10, 0.95)';
        navbar.style.boxShadow = '0 4px 20px rgba(0, 0, 0, 0.3)';
    } else {
        navbar.style.background = 'rgba(10, 10, 10, 0.8)';
        navbar.style.boxShadow = 'none';
    }
    
    lastScroll = currentScroll;
});

// Intersection Observer for fade-in animations
const observerOptions = {
    threshold: 0.1,
    rootMargin: '0px 0px -100px 0px'
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
document.querySelectorAll('.feature-card').forEach((card, index) => {
    card.style.opacity = '0';
    card.style.transform = 'translateY(30px)';
    card.style.transition = `opacity 0.6s ease ${index * 0.1}s, transform 0.6s ease ${index * 0.1}s`;
    observer.observe(card);
});

// Add typing effect to hero code
function typeCode() {
    const codeElement = document.querySelector('.hero-code code');
    if (!codeElement) return;
    
    const code = codeElement.innerHTML;
    codeElement.innerHTML = '';
    codeElement.style.opacity = '1';
    
    let i = 0;
    const speed = 20;
    
    function type() {
        if (i < code.length) {
            codeElement.innerHTML += code.charAt(i);
            i++;
            setTimeout(type, speed);
        }
    }
    
    // Uncomment to enable typing effect
    // setTimeout(type, 1000);
}

// Initialize on page load
document.addEventListener('DOMContentLoaded', () => {
    // Add loaded class for animations
    document.body.classList.add('loaded');
    
    // Parallax effect for hero background
    window.addEventListener('scroll', () => {
        const scrolled = window.pageYOffset;
        const heroBackground = document.querySelector('.hero-background');
        if (heroBackground) {
            heroBackground.style.transform = `translateY(${scrolled * 0.5}px)`;
        }
    });
});

// Add keyboard navigation for code tabs
document.addEventListener('keydown', (e) => {
    const activeTab = document.querySelector('.tab-button.active');
    if (!activeTab) return;
    
    let nextTab = null;
    
    if (e.key === 'ArrowLeft') {
        nextTab = activeTab.previousElementSibling;
    } else if (e.key === 'ArrowRight') {
        nextTab = activeTab.nextElementSibling;
    }
    
    if (nextTab && nextTab.classList.contains('tab-button')) {
        nextTab.click();
        nextTab.focus();
    }
});

// Easter egg: Konami code
let konamiCode = [];
const konamiSequence = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a'];

document.addEventListener('keydown', (e) => {
    konamiCode.push(e.key);
    konamiCode = konamiCode.slice(-konamiSequence.length);
    
    if (konamiCode.join(',') === konamiSequence.join(',')) {
        activateEasterEgg();
    }
});

function activateEasterEgg() {
    document.body.style.animation = 'rainbow 2s linear infinite';
    
    const style = document.createElement('style');
    style.textContent = `
        @keyframes rainbow {
            0% { filter: hue-rotate(0deg); }
            100% { filter: hue-rotate(360deg); }
        }
    `;
    document.head.appendChild(style);
    
    setTimeout(() => {
        document.body.style.animation = '';
        style.remove();
    }, 10000);
}

// Performance optimization: lazy load images when implemented
function lazyLoadImages() {
    const images = document.querySelectorAll('img[data-src]');
    
    const imageObserver = new IntersectionObserver((entries, observer) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const img = entry.target;
                img.src = img.dataset.src;
                img.removeAttribute('data-src');
                imageObserver.unobserve(img);
            }
        });
    });
    
    images.forEach(img => imageObserver.observe(img));
}

// Initialize lazy loading
lazyLoadImages();

// Add a subtle cursor trail effect (optional, can be enabled)
let cursorTrail = [];
const trailLength = 10;

function createCursorTrail(e) {
    const trail = document.createElement('div');
    trail.className = 'cursor-trail';
    trail.style.left = e.clientX + 'px';
    trail.style.top = e.clientY + 'px';
    document.body.appendChild(trail);
    
    cursorTrail.push(trail);
    
    if (cursorTrail.length > trailLength) {
        const oldTrail = cursorTrail.shift();
        oldTrail.remove();
    }
    
    setTimeout(() => {
        trail.style.opacity = '0';
        setTimeout(() => trail.remove(), 500);
    }, 500);
}

// Uncomment to enable cursor trail
// document.addEventListener('mousemove', createCursorTrail);

// Analytics placeholder (replace with actual analytics)
function trackEvent(category, action, label) {
    console.log('Event tracked:', category, action, label);
    // Add your analytics tracking code here
    // Example: gtag('event', action, { 'event_category': category, 'event_label': label });
}

// Track CTA button clicks
document.querySelectorAll('.btn-primary').forEach(btn => {
    btn.addEventListener('click', () => {
        trackEvent('CTA', 'click', btn.textContent);
    });
});


