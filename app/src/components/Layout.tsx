import type { ReactNode } from 'react';
import { useLocation } from 'react-router-dom';
import Navbar from './Navbar';
import Footer from './Footer';

interface LayoutProps {
  children: ReactNode;
  hideNav?: boolean;
  hideFooter?: boolean;
  landing?: boolean;
}

export default function Layout({ children, hideNav = false, hideFooter = false, landing = false }: LayoutProps) {
  const location = useLocation();
  const isLanding = landing || location.pathname === '/';

  return (
    <div className="min-h-[100dvh] bg-bg-base text-text-primary flex flex-col">
      {!hideNav && <Navbar />}
      <main className={isLanding ? '' : 'pt-16'}>{children}</main>
      {!hideFooter && <Footer />}
    </div>
  );
}
