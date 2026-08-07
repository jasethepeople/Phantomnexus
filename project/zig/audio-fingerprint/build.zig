const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.option(std.builtin.OptimizeMode, "optimize", "Build optimization mode") orelse .ReleaseFast;

    // Static library
    const lib = b.addStaticLibrary(.{
        .name = "audio_fingerprint",
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    lib.linkLibC();
    b.installArtifact(lib);

    // Tests
    const lib_tests = b.addTest(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    lib_tests.linkLibC();

    const run_tests = b.addRunArtifact(lib_tests);
    const test_step = b.step("test", "Run library tests");
    test_step.dependOn(&run_tests.step);

    // C header generation step (emitting to zig-out/include)
    const install_header = b.addInstallFile(
        lib.getEmittedH(),
        "include/audio_fingerprint.h",
    );
    b.getInstallStep().dependOn(&install_header.step);
}
