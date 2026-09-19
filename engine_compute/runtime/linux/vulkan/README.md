# Linux Vulkan loader policy

Dogwood does not currently ship a Linux `libvulkan.so.1` asset. The CLI leaves
Linux packages dependent on the user's system Vulkan loader and reports that
policy during packaging. This avoids distributing an unverified loader across
Linux distributions and architectures; the installed GPU driver remains
responsible for the Vulkan ICD.
