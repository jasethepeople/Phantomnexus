import { useEffect, useRef, useState, lazy, Suspense } from 'react';
import { Link } from 'react-router-dom';
import {
  Shield, Play, DollarSign, AlertTriangle, Music,
  Check, Eye, FileText, BarChart3, Wand2, Clock, History, Zap,
  ArrowRight
} from 'lucide-react';
import { motion, useInView } from 'framer-motion';

/* ------------------------------------------------------------------ */
/*  Lazy GSAP — isolated from Framer Motion tree                       */
/* ------------------------------------------------------------------ */
const ParticleField = lazy(() => import('../components/ParticleField'));

/* ------------------------------------------------------------------ */
/*  Animation helpers                                                 */
/* ------------------------------------------------------------------ */
const snap = [0.16, 1, 0.3, 1] as [number, number, number, number];

function useCountUp(end: number, duration = 1.2, start = 0) {
  const [val, setVal] = useState(start);
  const ref = useRef<HTMLSpanElement>(null);
  const inView = useInView(ref, { once: true });

  useEffect(() => {
    if (!inView) return;
    let raf: number;
    const t0 = performance.now();
    const tick = (t: number) => {
      const p = Math.min((t - t0) / (duration * 1000), 1);
      const eased = 1 - Math.pow(1 - p, 3);
      setVal(Math.round(start + (end - start) * eased));
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [inView, end, duration, start]);

  return { ref, val };
}

function FadeIn({
  children,
  className = '',
  delay = 0,
  y = 30,
  duration = 0.6,
}: {
  children: React.ReactNode;
  className?: string;
  delay?: number;
  y?: number;
  duration?: number;
}) {
  const ref = useRef(null);
  const inView = useInView(ref, { once: true, margin: '-50px' });
  return (
    <motion.div
      ref={ref}
      initial={{ opacity: 0, y }}
      animate={inView ? { opacity: 1, y: 0 } : { opacity: 0, y }}
      transition={{ duration, delay, ease: snap }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

function StaggerContainer({
  children,
  className = '',
  stagger = 0.06,
}: {
  children: React.ReactNode;
  className?: string;
  stagger?: number;
}) {
  const ref = useRef(null);
  const inView = useInView(ref, { once: true, margin: '-50px' });
  return (
    <motion.div
      ref={ref}
      initial="hidden"
      animate={inView ? 'visible' : 'hidden'}
      variants={{
        hidden: {},
        visible: { transition: { staggerChildren: stagger } },
      }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

const staggerChild = {
  hidden: { opacity: 0, y: 30 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.5, ease: snap } },
};

/* ------------------------------------------------------------------ */
/*  Section 1 — Hero                                                  */
/* ------------------------------------------------------------------ */
function HeroSection() {
  const c1 = useCountUp(50, 1.2);
  const c2 = useCountUp(99.7, 1.2);
  const c3 = useCountUp(2.3, 1.2);

  return (
    <section className="relative min-h-[100dvh] flex flex-col items-center justify-center overflow-hidden bg-bg-base pt-16">
      {/* Ambient orbs */}
      <div
        className="absolute top-0 right-0 w-[600px] h-[400px] pointer-events-none"
        style={{ background: 'radial-gradient(ellipse 600px 400px, rgba(124,92,255,0.08), transparent)' }}
      />
      <div
        className="absolute bottom-0 left-0 w-[500px] h-[500px] pointer-events-none"
        style={{ background: 'radial-gradient(ellipse 500px 500px, rgba(99,102,241,0.05), transparent)' }}
      />

      {/* Subtle grid overlay */}
      <div
        className="absolute inset-0 pointer-events-none opacity-[0.02]"
        style={{
          backgroundImage:
            'repeating-linear-gradient(0deg, transparent, transparent 59px, rgba(124,92,255,0.3) 60px), repeating-linear-gradient(90deg, transparent, transparent 59px, rgba(124,92,255,0.3) 60px)',
        }}
      />

      {/* Particle field */}
      <Suspense fallback={null}>
        <ParticleField />
      </Suspense>

      <div className="relative z-10 flex flex-col items-center text-center px-6 max-w-[900px] mx-auto">
        {/* Headline */}
        <motion.h1
          className="text-display-xl text-text-primary max-w-[800px]"
          initial={{ opacity: 0, y: 30 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.7, ease: snap }}
        >
          <span className="block">Upload with Confidence.</span>
          <motion.span
            className="block text-accent-primary"
            initial={{ opacity: 0, y: 30 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.7, delay: 0.15, ease: snap }}
          >
            Every Video, Analyzed.
          </motion.span>
        </motion.h1>

        {/* Subheadline */}
        <motion.p
          className="mt-6 text-body-lg text-text-secondary max-w-[560px]"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.4, ease: snap }}
        >
          YouTube Sentinel scans your videos before upload — predicting demonetization risks,
          flagging guideline violations, and catching copyright issues before they cost you.
        </motion.p>

        {/* CTA Group */}
        <motion.div
          className="mt-10 flex flex-col sm:flex-row items-center gap-4"
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.5, delay: 0.6, ease: snap }}
        >
          <Link
            to="/dashboard"
            className="px-8 py-3.5 bg-accent-primary text-text-inverse text-body font-semibold rounded-button hover:scale-[1.02] hover:shadow-glow-primary active:scale-[0.98] transition-all duration-200"
          >
            Start Free Analysis
          </Link>
          <a
            href="#how-it-works"
            className="flex items-center gap-2 px-6 py-3.5 text-body font-medium text-text-secondary hover:text-text-primary transition-colors duration-150"
          >
            <Play className="w-4 h-4" />
            See How It Works
          </a>
        </motion.div>

        {/* Trust Bar */}
        <motion.div
          className="mt-16 flex flex-col sm:flex-row items-center gap-6 sm:gap-0"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, delay: 0.8, ease: snap }}
        >
          {[
            { ref: c1.ref, val: `${c1.val}K+`, label: 'Videos Analyzed' },
            { ref: c2.ref, val: `${c2.val.toFixed(1)}%`, label: 'Accuracy Rate' },
            { ref: c3.ref, val: `${c3.val}M+`, label: 'Issues Caught' },
          ].map((metric, i) => (
            <div key={metric.label} className="flex items-center">
              <div className="flex flex-col items-center px-8 sm:px-12">
                <span ref={metric.ref} className="text-metric font-mono text-text-primary">
                  {metric.val}
                </span>
                <span className="text-caption text-text-muted mt-1">{metric.label}</span>
              </div>
              {i < 2 && (
                <div className="hidden sm:block w-px h-12 bg-border-default" />
              )}
            </div>
          ))}
        </motion.div>
      </div>

      {/* Hero Mockup */}
      <motion.div
        className="relative z-10 mt-12 px-6 max-w-[1000px] w-full mx-auto"
        initial={{ opacity: 0, y: 40, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.8, delay: 1.0, ease: snap }}
      >
        <img
          src="/hero-dashboard-mockup.png"
          alt="YouTube Sentinel Dashboard"
          className="w-full animate-float"
          style={{
            transform: 'perspective(1200px) rotateX(4deg)',
            boxShadow: '0 40px 80px rgba(0,0,0,0.5), 0 0 60px rgba(124,92,255,0.1)',
            borderRadius: '14px',
          }}
        />
      </motion.div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 2 — Problem Statement                                     */
/* ------------------------------------------------------------------ */
const problems = [
  {
    icon: DollarSign,
    iconColor: 'text-risk-warn',
    iconBg: 'bg-risk-warn/10',
    title: 'Silent Demonetization',
    body: "Your video goes live, views climb, but revenue stays at zero. YouTube doesn't always notify you — and by the time you notice, the algorithm has already stopped recommending your content.",
  },
  {
    icon: AlertTriangle,
    iconColor: 'text-risk-danger',
    iconBg: 'bg-risk-danger/10',
    title: 'Community Guideline Strikes',
    body: 'A single missed frame, one word, or a brief clip can trigger a strike. Three strikes and your channel is gone — along with years of work and your audience.',
  },
  {
    icon: Music,
    iconColor: 'text-risk-info',
    iconBg: 'bg-risk-info/10',
    title: 'Copyright Claims',
    body: 'Background music, stock footage, or even ambient audio can trigger Content ID claims. Revenue diverted, video blocked in regions, or removed entirely.',
  },
];

function ProblemSection() {
  return (
    <section className="bg-bg-base py-24 px-6">
      <div className="max-w-content mx-auto">
        <FadeIn>
          <span className="text-caption uppercase text-accent-primary tracking-[0.1em]">
            THE PROBLEM
          </span>
        </FadeIn>
        <FadeIn delay={0.1}>
          <h2 className="text-display-md text-text-primary mt-4 max-w-[700px]">
            One flagged video can destroy a month&apos;s revenue
          </h2>
        </FadeIn>

        <StaggerContainer className="mt-16 grid grid-cols-1 md:grid-cols-3 gap-6" stagger={0.12}>
          {problems.map((p) => (
            <motion.div
              key={p.title}
              variants={staggerChild}
              className="bg-bg-surface border border-border-default rounded-card p-8 hover:border-border-hover hover:-translate-y-0.5 hover:shadow-card-hover transition-all duration-300"
            >
              <div className={`w-16 h-16 rounded-full ${p.iconBg} flex items-center justify-center mb-6`}>
                <p.icon className={`w-8 h-8 ${p.iconColor}`} />
              </div>
              <h3 className="text-heading-lg text-text-primary">{p.title}</h3>
              <p className="mt-3 text-body-sm text-text-secondary leading-relaxed">{p.body}</p>
            </motion.div>
          ))}
        </StaggerContainer>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 3 — How It Works                                          */
/* ------------------------------------------------------------------ */
const steps = [
  {
    num: '01',
    title: 'Upload Your Video',
    body: 'Drag and drop or select your video file. We support all major formats up to 4K resolution. Your video is processed securely and never stored longer than necessary.',
    features: ['MP4, MOV, MKV, AVI supported', 'Up to 10GB per file', 'End-to-end encrypted upload'],
    image: '/feature-upload.png',
  },
  {
    num: '02',
    title: 'AI-Powered Deep Analysis',
    body: "Our engine scans every frame, every audio segment, and every subtitle. We check against YouTube's policies, advertiser-friendly content guidelines, and copyright databases — all in under 3 minutes.",
    features: ['Visual content analysis (frames, objects, text)', 'Audio analysis (music, speech, sound effects)', 'Real-time progress with live updates'],
    image: '/feature-analysis.png',
  },
  {
    num: '03',
    title: 'Fix Issues & Publish Safely',
    body: 'Review flagged issues with detailed explanations, apply one-click auto-fixes for common problems, and re-analyze to confirm your video is clean. Publish with total confidence.',
    features: ['Detailed violation reports with timestamps', 'One-click auto-fix suggestions', 'Re-analysis to verify fixes'],
    image: '/feature-fix.png',
  },
];

function HowItWorksSection() {
  return (
    <section id="how-it-works" className="bg-bg-surface py-30 px-6">
      <div className="max-w-content mx-auto">
        <FadeIn className="text-center">
          <span className="text-caption uppercase text-accent-primary tracking-[0.1em]">
            HOW IT WORKS
          </span>
        </FadeIn>
        <FadeIn className="text-center mt-4" delay={0.1}>
          <h2 className="text-display-md text-text-primary">Three steps to bulletproof uploads</h2>
        </FadeIn>

        <div className="mt-20 max-w-[1100px] mx-auto space-y-24">
          {steps.map((step, i) => {
            const isEven = i % 2 === 1;
            return (
              <FadeIn key={step.num} y={40}>
                <div
                  className={`flex flex-col ${isEven ? 'lg:flex-row-reverse' : 'lg:flex-row'} items-center gap-12 lg:gap-16`}
                >
                  {/* Content */}
                  <div className="flex-1">
                    <span className="text-display-xl text-accent-primary/20 font-extrabold">
                      {step.num}
                    </span>
                    <h3 className="text-heading-xl text-text-primary mt-2">{step.title}</h3>
                    <p className="mt-4 text-body text-text-secondary leading-relaxed">{step.body}</p>
                    <ul className="mt-4 space-y-2">
                      {step.features.map((f) => (
                        <li key={f} className="flex items-center gap-2.5 text-body-sm text-text-secondary">
                          <Check className="w-4 h-4 text-risk-safe flex-shrink-0" />
                          {f}
                        </li>
                      ))}
                    </ul>
                  </div>
                  {/* Image */}
                  <div className="flex-1">
                    <img
                      src={step.image}
                      alt={step.title}
                      className="w-full rounded-card border border-border-default shadow-lg"
                    />
                  </div>
                </div>
              </FadeIn>
            );
          })}
        </div>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 4 — Live Analysis Demo                                    */
/* ------------------------------------------------------------------ */
function LiveDemoSection() {
  const [progress, setProgress] = useState(0);
  const ref = useRef(null);
  const inView = useInView(ref, { once: true, margin: '-100px' });

  useEffect(() => {
    if (!inView) return;
    let raf: number;
    const t0 = performance.now();
    const tick = (t: number) => {
      const p = Math.min((t - t0) / 2000, 1);
      const eased = 1 - Math.pow(1 - p, 3);
      setProgress(Math.round(67 * eased));
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [inView]);

  const circumference = 2 * Math.PI * 54;
  const offset = circumference - (progress / 100) * circumference;

  return (
    <section className="bg-bg-base py-30 px-6">
      <div className="max-w-content mx-auto">
        <FadeIn>
          <span className="text-caption uppercase text-accent-primary tracking-[0.1em]">LIVE DEMO</span>
        </FadeIn>
        <FadeIn delay={0.1}>
          <h2 className="text-display-md text-text-primary mt-4">See the analysis in action</h2>
        </FadeIn>

        <FadeIn delay={0.2} y={40}>
          <div
            ref={ref}
            className="mt-16 max-w-[1100px] mx-auto bg-bg-surface border border-border-default rounded-card p-6 md:p-8"
          >
            {/* Top bar */}
            <div className="flex items-center justify-between flex-wrap gap-4">
              <div className="flex items-center gap-4">
                <div className="w-32 h-20 bg-bg-surface-raised rounded-card-sm flex items-center justify-center border border-border-default">
                  <Play className="w-8 h-8 text-text-muted" />
                </div>
                <div>
                  <p className="text-body font-medium text-text-primary">my_vlog_episode_42.mp4</p>
                  <p className="text-caption text-text-muted">12:34</p>
                </div>
              </div>
              <div className="flex items-center gap-2 px-3 py-1.5 bg-accent-primary-dim rounded-badge border border-accent-primary/20">
                <span className="w-2 h-2 rounded-full bg-accent-primary animate-pulse-dot" />
                <span className="text-caption text-accent-primary font-medium">Analyzing...</span>
              </div>
            </div>

            {/* Progress ring */}
            <div className="mt-8 flex flex-col items-center">
              <div className="relative w-[120px] h-[120px]">
                <svg className="w-full h-full -rotate-90" viewBox="0 0 120 120">
                  <circle cx="60" cy="60" r="54" fill="none" stroke="#181920" strokeWidth="8" />
                  <circle
                    cx="60"
                    cy="60"
                    r="54"
                    fill="none"
                    stroke="url(#brandGrad)"
                    strokeWidth="8"
                    strokeLinecap="round"
                    strokeDasharray={circumference}
                    strokeDashoffset={offset}
                    style={{ transition: 'stroke-dashoffset 0.1s linear' }}
                  />
                  <defs>
                    <linearGradient id="brandGrad" x1="0%" y1="0%" x2="100%" y2="100%">
                      <stop offset="0%" stopColor="#7C5CFF" />
                      <stop offset="50%" stopColor="#6366F1" />
                      <stop offset="100%" stopColor="#22D3EE" />
                    </linearGradient>
                  </defs>
                </svg>
                <div className="absolute inset-0 flex items-center justify-center">
                  <span className="text-metric-sm font-mono text-text-primary">{progress}%</span>
                </div>
              </div>

              {/* Step indicators */}
              <div className="mt-6 flex flex-wrap items-center justify-center gap-4 md:gap-8">
                {[
                  { label: 'Video Processing', done: true },
                  { label: 'Visual Analysis', done: true },
                  { label: 'Audio Analysis', active: true },
                  { label: 'Report Generation', done: false },
                ].map((s) => (
                  <div key={s.label} className="flex items-center gap-2">
                    {s.done ? (
                      <div className="w-5 h-5 rounded-full bg-risk-safe flex items-center justify-center">
                        <Check className="w-3 h-3 text-white" />
                      </div>
                    ) : s.active ? (
                      <div className="w-5 h-5 rounded-full border-2 border-accent-primary flex items-center justify-center">
                        <div className="w-2 h-2 rounded-full bg-accent-primary animate-pulse-dot" />
                      </div>
                    ) : (
                      <div className="w-5 h-5 rounded-full border-2 border-border-default" />
                    )}
                    <span className={`text-caption ${s.done || s.active ? 'text-text-primary' : 'text-text-muted'}`}>
                      {s.label}
                    </span>
                  </div>
                ))}
              </div>
            </div>

            {/* Live findings */}
            <div className="mt-8 bg-bg-surface-raised rounded-card p-6 border border-border-default">
              <div className="flex items-center gap-3 mb-4">
                <span className="text-heading-md text-text-primary font-semibold">Live Findings</span>
                <span className="px-2.5 py-0.5 bg-risk-warn/10 text-risk-warn text-caption rounded-badge border border-risk-warn/20">
                  3 issues detected
                </span>
              </div>

              <div className="space-y-3">
                {[
                  { severity: 'Warning', color: 'text-risk-warn', bg: 'bg-risk-warn/10', border: 'border-risk-warn/20', text: 'Detected music track at 03:22 — potential copyright claim' },
                  { severity: 'Critical', color: 'text-risk-danger', bg: 'bg-risk-danger/10', border: 'border-risk-danger/20', text: 'Visual contains flagged content category at 07:15' },
                  { severity: 'Info', color: 'text-risk-info', bg: 'bg-risk-info/10', border: 'border-risk-info/20', text: 'Title keywords may trigger limited ads' },
                ].map((issue, i) => (
                  <motion.div
                    key={i}
                    initial={{ opacity: 0, y: 20 }}
                    animate={inView ? { opacity: 1, y: 0 } : {}}
                    transition={{ duration: 0.5, delay: 0.8 + i * 0.8, ease: snap }}
                    className={`flex items-center gap-3 p-3 rounded-card-sm ${issue.bg} border ${issue.border}`}
                  >
                    <AlertTriangle className={`w-5 h-5 ${issue.color} flex-shrink-0`} />
                    <span className="text-body-sm text-text-primary flex-1">{issue.text}</span>
                    <span className={`text-caption ${issue.color} font-medium px-2 py-0.5 rounded-badge ${issue.bg} ${issue.border} border`}>
                      {issue.severity}
                    </span>
                  </motion.div>
                ))}
              </div>
            </div>
          </div>
        </FadeIn>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 5 — Feature Grid                                          */
/* ------------------------------------------------------------------ */
const features = [
  { icon: Eye, color: 'text-accent-primary', bg: 'bg-accent-primary/10', title: 'Visual Content Scan', desc: "Frame-by-frame analysis detects inappropriate visuals, on-screen text, and brand safety issues before YouTube's reviewers see them." },
  { icon: Music, color: 'text-risk-info', bg: 'bg-risk-info/10', title: 'Audio & Music Detection', desc: 'Identifies copyrighted music, sound effects, and speech patterns that could trigger Content ID claims or limited ads.' },
  { icon: FileText, color: 'text-risk-warn', bg: 'bg-risk-warn/10', title: 'Policy Violation Check', desc: "Cross-references against YouTube's Community Guidelines, Advertiser-Friendly Content policies, and regional restrictions." },
  { icon: BarChart3, color: 'text-risk-safe', bg: 'bg-risk-safe/10', title: 'Risk Scoring System', desc: 'Every video gets an overall risk score from 0-100, plus individual category scores so you know exactly what needs attention.' },
  { icon: Wand2, color: 'text-accent-primary', bg: 'bg-accent-primary/10', title: 'Auto-Fix Suggestions', desc: 'One-click recommendations to resolve common issues — from muting flagged audio segments to suggesting title rewording.' },
  { icon: Clock, color: 'text-risk-info', bg: 'bg-risk-info/10', title: 'Real-Time Analysis', desc: 'Watch the analysis happen live with a detailed progress breakdown. Most videos analyzed in under 3 minutes.' },
  { icon: Shield, color: 'text-risk-safe', bg: 'bg-risk-safe/10', title: 'Demonetization Shield', desc: 'Predictive model trained on thousands of demonetized videos to identify patterns that lead to limited or no ads.' },
  { icon: History, color: 'text-risk-warn', bg: 'bg-risk-warn/10', title: 'Analysis History', desc: 'Full history of all your analyzed videos with searchable reports, trend tracking, and comparison tools.' },
  { icon: Zap, color: 'text-accent-primary', bg: 'bg-accent-primary/10', title: 'Batch Processing', desc: 'Upload and analyze multiple videos simultaneously. Perfect for content creators with publishing schedules.' },
];

function FeatureGridSection() {
  return (
    <section id="features" className="bg-bg-surface py-30 px-6">
      <div className="max-w-content mx-auto">
        <FadeIn>
          <span className="text-caption uppercase text-accent-primary tracking-[0.1em]">FEATURES</span>
        </FadeIn>
        <FadeIn delay={0.1}>
          <h2 className="text-display-md text-text-primary mt-4">Everything you need to stay protected</h2>
        </FadeIn>

        <StaggerContainer className="mt-16 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6" stagger={0.06}>
          {features.map((f) => (
            <motion.div
              key={f.title}
              variants={staggerChild}
              whileHover={{ y: -4, borderColor: 'var(--border-hover)' }}
              className="bg-bg-surface-raised border border-border-default rounded-card p-7 hover:shadow-feature-hover transition-all duration-300 cursor-default"
            >
              <div className={`w-14 h-14 rounded-full ${f.bg} flex items-center justify-center`}>
                <f.icon className={`w-6 h-6 ${f.color}`} />
              </div>
              <h3 className="text-heading-md text-text-primary mt-5">{f.title}</h3>
              <p className="mt-2 text-body-sm text-text-secondary leading-relaxed">{f.desc}</p>
            </motion.div>
          ))}
        </StaggerContainer>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 6 — Testimonials                                          */
/* ------------------------------------------------------------------ */
const testimonials = [
  {
    quote: "Sentinel caught a copyright issue in my intro music that would've cost me the entire video's revenue. Saved me literally thousands of dollars.",
    name: 'Alex Chen',
    role: 'Tech Creator, 2.4M subs',
    gradient: 'from-accent-primary to-risk-info',
  },
  {
    quote: "I upload daily. Before Sentinel, I was constantly anxious about strikes. Now I analyze every video first and haven't had a single issue in 8 months.",
    name: 'Maya Rodriguez',
    role: 'Lifestyle Vlogger, 890K subs',
    gradient: 'from-risk-warn to-risk-danger',
  },
  {
    quote: 'The risk score system is brilliant. I can see exactly what\'s wrong and fix it in minutes. My ad revenue is up 40% since I started using it.',
    name: 'Jordan Park',
    role: 'Gaming Channel, 1.7M subs',
    gradient: 'from-risk-safe to-accent-primary',
  },
];

function StarRating() {
  return (
    <div className="flex gap-1">
      {[...Array(5)].map((_, i) => (
        <motion.svg
          key={i}
          initial={{ opacity: 0, scale: 0 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.3, delay: i * 0.1, ease: snap }}
          className="w-4 h-4 text-[#FBBF24]"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
        </motion.svg>
      ))}
    </div>
  );
}

function TestimonialsSection() {
  return (
    <section className="bg-bg-base py-30 px-6">
      <div className="max-w-content mx-auto">
        <FadeIn>
          <span className="text-caption uppercase text-accent-primary tracking-[0.1em]">TRUSTED BY CREATORS</span>
        </FadeIn>
        <FadeIn delay={0.1}>
          <h2 className="text-display-md text-text-primary mt-4">Creators who sleep better at night</h2>
        </FadeIn>

        <StaggerContainer className="mt-16 grid grid-cols-1 md:grid-cols-3 gap-6" stagger={0.15}>
          {testimonials.map((t) => (
            <motion.div
              key={t.name}
              variants={staggerChild}
              className="bg-bg-surface border border-border-default rounded-card p-8 hover:border-border-hover transition-colors duration-300"
            >
              <StarRating />
              <p className="mt-4 text-body text-text-primary italic leading-relaxed">&ldquo;{t.quote}&rdquo;</p>
              <div className="mt-6 flex items-center gap-3">
                <div className={`w-10 h-10 rounded-full bg-gradient-to-br ${t.gradient}`} />
                <div>
                  <p className="text-heading-md text-text-primary font-semibold">{t.name}</p>
                  <p className="text-caption text-text-muted">{t.role}</p>
                </div>
              </div>
            </motion.div>
          ))}
        </StaggerContainer>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 7 — Pricing Teaser                                        */
/* ------------------------------------------------------------------ */
function PricingTeaserSection() {
  return (
    <section className="bg-bg-surface py-24 px-6">
      <div className="max-w-content mx-auto text-center">
        <FadeIn>
          <h2 className="text-display-md text-text-primary">Start free. Scale when you need to.</h2>
        </FadeIn>
        <FadeIn delay={0.15}>
          <p className="mt-4 text-body-lg text-text-secondary max-w-[500px] mx-auto">
            Analyze your first 5 videos completely free. No credit card required.
          </p>
        </FadeIn>
        <FadeIn delay={0.3}>
          <div className="mt-10 flex flex-col items-center">
            <Link
              to="/pricing"
              className="inline-flex items-center gap-2 px-8 py-3.5 bg-accent-primary text-text-inverse text-body font-semibold rounded-button hover:scale-[1.02] hover:shadow-glow-primary active:scale-[0.98] transition-all duration-200"
            >
              View Pricing Plans
              <ArrowRight className="w-5 h-5" />
            </Link>
            <p className="mt-4 text-caption text-text-muted">
              Free forever plan available. Upgrade anytime.
            </p>
          </div>
        </FadeIn>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Section 8 — CTA + Footer area (CTA only; Footer in Layout)         */
/* ------------------------------------------------------------------ */
function CTASection() {
  return (
    <section className="relative bg-bg-base py-30 px-6 overflow-hidden">
      {/* Subtle gradient orb */}
      <div
        className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[800px] h-[400px] pointer-events-none animate-gradient-orb"
        style={{ background: 'radial-gradient(ellipse 800px 400px, rgba(124,92,255,0.06), transparent)' }}
      />

      <div className="relative z-10 max-w-content mx-auto text-center">
        <FadeIn>
          <h2 className="text-display-lg text-text-primary">
            <span className="block">Stop guessing. Start analyzing.</span>
            <span className="block text-accent-primary mt-2">Your channel will thank you.</span>
          </h2>
        </FadeIn>
        <FadeIn delay={0.15}>
          <p className="mt-5 text-body-lg text-text-secondary max-w-[500px] mx-auto">
            Join 50,000+ creators who analyze before they upload. Your first 5 videos are on us.
          </p>
        </FadeIn>
        <FadeIn delay={0.3}>
          <div className="mt-12 flex flex-col sm:flex-row items-center justify-center gap-4">
            <Link
              to="/dashboard"
              className="px-8 py-3.5 bg-accent-primary text-text-inverse text-body font-semibold rounded-button hover:scale-[1.02] hover:shadow-glow-primary active:scale-[0.98] transition-all duration-200"
            >
              Get Started Free
            </Link>
            <button className="flex items-center gap-2 px-6 py-3.5 bg-bg-surface-raised border border-border-default text-text-primary text-body font-medium rounded-button hover:border-border-hover hover:bg-bg-surface-hover transition-all duration-200">
              <Play className="w-4 h-4" />
              Watch Demo
            </button>
          </div>
        </FadeIn>
      </div>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Home Page                                                         */
/* ------------------------------------------------------------------ */
export default function Home() {
  return (
    <>
      <HeroSection />
      <ProblemSection />
      <HowItWorksSection />
      <LiveDemoSection />
      <FeatureGridSection />
      <TestimonialsSection />
      <PricingTeaserSection />
      <CTASection />
    </>
  );
}
