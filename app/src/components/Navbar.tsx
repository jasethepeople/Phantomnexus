import { useState, useEffect } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { Menu, X, Shield } from 'lucide-react';

interface NavLink {
  label: string;
  href?: string;
  to?: string;
}

const navLinks: NavLink[] = [
  { label: 'Features', href: '/#features' },
  { label: 'How It Works', href: '/#how-it-works' },
  { label: 'Pricing', to: '/pricing' },
];

export default function Navbar() {
  const [scrolled, setScrolled] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const location = useLocation();

  useEffect(() => {
    const handleScroll = () => setScrolled(window.scrollY > 50);
    window.addEventListener('scroll', handleScroll, { passive: true });
    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    setMobileOpen(false);
  }, [location.pathname]);

  const isLanding = location.pathname === '/';

  return (
    <nav
      className={
        'fixed top-0 left-0 right-0 z-50 h-16 flex items-center transition-all duration-300' +
        (scrolled
          ? ' bg-bg-surface/80 backdrop-blur-[12px] border-b border-border-default'
          : ' bg-transparent')
      }
    >
      <div className="max-w-content mx-auto w-full px-6 flex items-center justify-between">
        {/* Logo */}
        <Link to="/" className="flex items-center gap-2.5 group">
          <Shield className="w-7 h-7 text-accent-primary group-hover:scale-105 transition-transform" />
          <span className="text-heading-md font-bold text-accent-primary tracking-tight">
            Sentinel
          </span>
        </Link>

        {/* Center Nav Links — desktop */}
        <div className="hidden md:flex items-center gap-8">
          {navLinks.map((link) =>
            link.to ? (
              <Link
                key={link.label}
                to={link.to}
                className="text-body-sm font-medium text-text-secondary hover:text-text-primary transition-colors duration-150"
              >
                {link.label}
              </Link>
            ) : (
              <a
                key={link.label}
                href={isLanding ? (link.href ?? '#').replace('/#', '#') : link.href}
                className="text-body-sm font-medium text-text-secondary hover:text-text-primary transition-colors duration-150"
              >
                {link.label}
              </a>
            )
          )}
        </div>

        {/* Right Actions — desktop */}
        <div className="hidden md:flex items-center gap-3">
          <Link
            to="/dashboard"
            className="px-5 py-2 text-body-sm font-medium text-text-secondary hover:text-text-primary transition-colors duration-150"
          >
            Log In
          </Link>
          <Link
            to="/dashboard"
            className="px-5 py-2 bg-accent-primary text-text-inverse text-body-sm font-semibold rounded-button hover:scale-[1.02] hover:shadow-glow-primary active:scale-[0.98] transition-all duration-200"
          >
            Get Started
          </Link>
        </div>

        {/* Mobile hamburger */}
        <button
          className="md:hidden text-text-primary p-2"
          onClick={() => setMobileOpen(!mobileOpen)}
          aria-label="Toggle menu"
        >
          {mobileOpen ? <X className="w-6 h-6" /> : <Menu className="w-6 h-6" />}
        </button>
      </div>

      {/* Mobile overlay */}
      {mobileOpen && (
        <div className="fixed inset-0 top-16 bg-bg-base/95 backdrop-blur-lg z-40 md:hidden flex flex-col items-center pt-16 gap-8">
          {navLinks.map((link, i) =>
            link.to ? (
              <Link
                key={link.label}
                to={link.to}
                onClick={() => setMobileOpen(false)}
                className="text-heading-lg text-text-primary hover:text-accent-primary transition-colors duration-150"
                style={{ animationDelay: `${i * 0.06}s` }}
              >
                {link.label}
              </Link>
            ) : (
              <a
                key={link.label}
                href={isLanding ? (link.href ?? '#').replace('/#', '#') : link.href}
                onClick={() => {
                  setMobileOpen(false);
                  const href = link.href ?? '';
                  if (isLanding && href.startsWith('/#')) {
                    setTimeout(() => {
                      const el = document.getElementById(href.replace('/#', ''));
                      el?.scrollIntoView({ behavior: 'smooth' });
                    }, 100);
                  }
                }}
                className="text-heading-lg text-text-primary hover:text-accent-primary transition-colors duration-150"
                style={{ animationDelay: `${i * 0.06}s` }}
              >
                {link.label}
              </a>
            )
          )}
          <div className="flex flex-col items-center gap-4 mt-8">
            <Link
              to="/dashboard"
              className="text-body-lg text-text-secondary hover:text-text-primary transition-colors"
              onClick={() => setMobileOpen(false)}
            >
              Log In
            </Link>
            <Link
              to="/dashboard"
              className="px-8 py-3 bg-accent-primary text-text-inverse text-body font-semibold rounded-button"
              onClick={() => setMobileOpen(false)}
            >
              Get Started
            </Link>
          </div>
        </div>
      )}
    </nav>
  );
}
