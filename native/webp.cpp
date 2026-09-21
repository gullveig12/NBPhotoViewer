#include "src/webp/decode.h"
#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <cstring>

struct NvImage { unsigned char *data; size_t length; uint32_t width, height; };
extern "C" int32_t nv_webp_scaled(const unsigned char *bytes, size_t length, uint32_t edge, NvImage *out) {
    std::memset(out, 0, sizeof(*out));
    WebPDecoderConfig config;
    if(!WebPInitDecoderConfig(&config) || !edge || edge > 1024) return -1;
    if(WebPGetFeatures(bytes, length, &config.input) != VP8_STATUS_OK || config.input.has_animation) return -1;
    uint32_t w=config.input.width, h=config.input.height;
    if(!w || !h || uint64_t(w)*h > 100000000) return -1;
    uint32_t longest=std::max(w,h);
    if(longest > edge) {
        w=std::max(1u,(uint32_t)(uint64_t(w)*edge/longest));
        h=std::max(1u,(uint32_t)(uint64_t(h)*edge/longest));
        config.options.use_scaling=1;
        config.options.scaled_width=w;config.options.scaled_height=h;
    }
    const size_t pixels=size_t(w)*h;
    auto data=static_cast<unsigned char*>(std::malloc(pixels*4));
    if(!data) return -1;
    config.output.colorspace=MODE_rgbA; // Premultiplied RGBA, filtered by libwebp.
    config.output.is_external_memory=1;
    config.output.u.RGBA.rgba=data;
    config.output.u.RGBA.stride=w*4;
    config.output.u.RGBA.size=pixels*4;
    const auto status=WebPDecode(bytes,length,&config);
    WebPFreeDecBuffer(&config.output);
    if(status != VP8_STATUS_OK) { std::free(data); return -1; }
    for(size_t i=0;i<pixels;i++) {
        unsigned char r=data[i*4],g=data[i*4+1],b=data[i*4+2],a=data[i*4+3];
        unsigned bg=(29*(255-a)+127)/255;
        data[i*3]=(unsigned char)std::min(255u,r+bg);
        data[i*3+1]=(unsigned char)std::min(255u,g+bg);
        data[i*3+2]=(unsigned char)std::min(255u,b+bg);
    }
    out->data=data;out->length=pixels*3;out->width=w;out->height=h;
    return 0;
}
