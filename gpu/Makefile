# Build the block-parallel codec.
#   make cpu_ref   -> host reference (any C++ compiler; runs on the Mac)
#   make gpu       -> CUDA build (needs nvcc; runs on the rented 4090)
#   make all       -> both (gpu only if nvcc is present)
CXX      ?= c++
NVCC     ?= nvcc
CUDA_ARCH ?= sm_89          # RTX 4090 = Ada = sm_89
CXXFLAGS ?= -O2 -std=c++14 -I src
NVFLAGS  ?= -O3 -std=c++14 -I src -arch=$(CUDA_ARCH) -Xcompiler -pthread

HDRS = src/common.cuh src/tables.hpp src/coder.cuh src/model.cuh \
       src/segment.cuh src/container.hpp

.PHONY: all clean
all: cpu_ref
	@command -v $(NVCC) >/dev/null 2>&1 && $(MAKE) gpu || \
	  echo "nvcc not found — skipping gpu target (expected off-GPU)"

cpu_ref: src/cpu_ref.cpp $(HDRS)
	$(CXX) $(CXXFLAGS) src/cpu_ref.cpp -o cpu_ref

gpu: src/main.cu $(HDRS)
	$(NVCC) $(NVFLAGS) src/main.cu -o gpu

clean:
	rm -f cpu_ref gpu
