
#version 450

in vec2 image_coord;
out vec4 f_color;

layout(binding=0) uniform sampler2D image_texture;
layout(binding=1) uniform sampler2D lut_texture;

const float LUT_MAX = float(1<<16) - 1.0;
const uint LOG_LUT_IMG_SIZE = 8; // The LUT-image is assumed to be 256x256 (=65536 entries)

void main() {
    float val = texture(image_texture, image_coord).r;
    uint stored_value = uint(val * LUT_MAX);

    uint y = stored_value >> LOG_LUT_IMG_SIZE;
    uint x = stored_value - (y << LOG_LUT_IMG_SIZE);
    ivec2 lut_coord = ivec2(int(x), int(y));
    float norm_luminance = texelFetch(lut_texture, lut_coord, 0).r;

    f_color = vec4(norm_luminance, norm_luminance, norm_luminance, 1.0);
}